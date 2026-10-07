//! Bounded text-only proposals. Canonical structure and native publication stay authoritative.
use crate::{
    App, auth, content,
    document::{Document, Node},
    error::{Error, Result},
    model::{Post, PostInput, Session},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MAX_SEGMENTS: usize = 192;
const CHUNK_BYTES: usize = 2048;
const MAX_TEXT: usize = 128 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub id: String,
    pub text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub source_id: String,
    pub source_version: i64,
    pub source_document_sha256: String,
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub segments: Vec<Segment>,
}
fn chunks(value: &str) -> Vec<&str> {
    let mut rest = value;
    let mut result = Vec::new();
    while !rest.is_empty() {
        let mut end = rest.len().min(CHUNK_BYTES);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        // Prefer a sentence or whitespace boundary; never discard the separator.
        if end < rest.len()
            && let Some((index, c)) = rest[..end].char_indices().rev().find(|(i, c)| {
                *i > end / 2 && (c.is_whitespace() || ['。', '！', '？'].contains(c))
            })
        {
            end = index + c.len_utf8();
        }
        result.push(&rest[..end]);
        rest = &rest[end..];
    }
    result
}
fn visit(node: &Node, path: &str, protected: bool, out: &mut Vec<Segment>) {
    let protected =
        protected || node.kind == "code_block" || node.marks.iter().any(|m| m.kind == "code");
    if !protected
        && let Some(text) = &node.text
        && !text.trim().is_empty()
    {
        for (part, text) in chunks(text).into_iter().enumerate() {
            out.push(Segment {
                id: format!("{path}:{part}"),
                text: text.into(),
            });
        }
    }
    for (index, child) in node.content.iter().enumerate() {
        visit(child, &format!("{path}/{index}"), protected, out);
    }
}
pub fn segments(document: &Document) -> Result<Vec<Segment>> {
    let mut out = Vec::new();
    visit(&document.root, "root", false, &mut out);
    if out.len() > MAX_SEGMENTS || out.iter().map(|s| s.text.len()).sum::<usize>() > MAX_TEXT {
        return Err(Error::invalid(
            "Translation exceeds 192 text segments or 128 KiB; divide the source document.",
        ));
    }
    Ok(out)
}
/// Exact complete segment set only. The model never submits a document tree.
pub fn replace(document: &Document, replacements: &[Segment]) -> Result<Document> {
    let expected = segments(document)?;
    if expected.len() != replacements.len() {
        return Err(Error::invalid(
            "Review every source segment before applying a complete proposal.",
        ));
    }
    let mut by_path = BTreeMap::<String, String>::new();
    let mut bytes = 0;
    for (source, replacement) in expected.iter().zip(replacements) {
        bytes += replacement.text.len();
        if source.id != replacement.id
            || replacement.text.trim().is_empty()
            || replacement.text.len() > CHUNK_BYTES * 4
            || bytes > 512 * 1024
            || replacement.text.contains('\0')
        {
            return Err(Error::invalid(
                "Invalid, reordered or oversized translated segment.",
            ));
        }
        let path = source.id.rsplit_once(':').unwrap().0;
        by_path
            .entry(path.into())
            .or_default()
            .push_str(&replacement.text);
    }
    fn apply(node: &mut Node, path: &str, values: &BTreeMap<String, String>) {
        if let Some(value) = values.get(path) {
            node.text = Some(value.clone());
        }
        for (index, child) in node.content.iter_mut().enumerate() {
            apply(child, &format!("{path}/{index}"), values);
        }
    }
    let mut result = document.clone();
    apply(&mut result.root, "root", &by_path);
    Document::parse(&result.encode())
}
pub fn manifest(source: &Post) -> Result<Value> {
    let document = Document::parse(&source.document)?;
    Ok(
        json!({"source_id":source.id,"source_version":source.version,"source_document_sha256":auth::digest(source.document.as_bytes()),"source_locale":source.locale,"translation_group":source.translation_group,"title":source.title,"segments":segments(&document)?,"limits":{"segments":MAX_SEGMENTS,"source_bytes":MAX_TEXT,"segment_bytes":CHUNK_BYTES},"boundary":"Text proposals only; structure, code, attributes and references are retained. Review language accuracy before publication."}),
    )
}
pub async fn apply(app: &App, actor: &Session, proposal: Proposal) -> Result<Post> {
    let guard = app.mutation().await?;
    auth::current_editor(app, actor).await?;
    uuid::Uuid::parse_str(&proposal.source_id).map_err(|_| Error::invalid("Use a source UUID."))?;
    let source = content::get(app, &proposal.source_id).await?;
    if source.version != proposal.source_version
        || auth::digest(source.document.as_bytes()) != proposal.source_document_sha256
    {
        return Err(Error::conflict());
    }
    crate::discovery::load(app)
        .await?
        .0
        .language(&proposal.locale)?;
    if source.locale == proposal.locale || source.translation_group.is_empty() {
        return Err(Error::invalid(
            "Choose another configured language and a source translation group.",
        ));
    }
    let restricted = sqlx::query(
        "SELECT policy_id FROM member_resources WHERE kind='post' AND resource_id=$1 LIMIT 1",
    )
    .bind(&source.id)
    .fetch_optional(&app.db.pool)
    .await?;
    if restricted.is_some() {
        return Err(Error::invalid(
            "Provision a protected translated target before transferring restricted content; this proposal creates only a new unprotected draft.",
        ));
    }
    let document = replace(&Document::parse(&source.document)?, &proposal.segments)?;
    let input = PostInput {
        csrf: String::new(),
        title: proposal.title,
        slug: proposal.slug,
        kind: source.kind,
        body: document.markdown(),
        document: document.encode(),
        locale: proposal.locale,
        translation_group: source.translation_group,
        fields: source.fields,
        blocks: "[]".into(),
        seo: "{}".into(),
        import_markdown: false,
        categories: String::new(),
        tags: String::new(),
        taxonomies: "{}".into(),
        version: 0,
        action: "save".into(),
        publish_at: 0,
    };
    let result = content::save_guarded(app, actor, None, input, None, &guard)
        .await?
        .post;
    tracing::debug!(
        event = "document_translation_draft",
        segments = proposal.segments.len(),
        source_version = source.version
    );
    Ok(result)
}
