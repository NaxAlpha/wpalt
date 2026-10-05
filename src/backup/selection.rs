//! Conservative editorial selection for a fresh-target recovery package.
//! Other domains remain intact; this never merges rows into a running site.
use super::{Envelope, validate};
use crate::{
    auth::digest,
    config::Config,
    error::{Error, Result},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

const CHILDREN: &[&str] = &[
    "revisions",
    "comments",
    "post_terms",
    "published_post_terms",
];
const SCAN_BYTES: usize = 64 * 1024 * 1024;
const EDGES: usize = 100_000;

pub struct Prepared {
    pub report: Value,
    pub bytes: Vec<u8>,
    pub plan: String,
}

/// Transform declared presentation fields only. Credentials, identities, queued
/// payloads and immutable financial records are never subject to text replacement.
pub fn clone_package(config: &Config, encoded: &[u8], source: &str) -> Result<Prepared> {
    let origin = url::Url::parse(source)
        .map_err(|_| Error::invalid("Use a complete source HTTP origin."))?;
    if !["http", "https"].contains(&origin.scheme())
        || origin.origin().ascii_serialization() != source
        || source == config.origin()
    {
        return Err(Error::invalid(
            "Use a distinct complete source HTTP origin without a path.",
        ));
    }
    let mut snapshot = validate(config, encoded)?;
    let target = config.origin();
    let mut changed = 0usize;
    fn rewrite(text: &str, source: &str, target: &str, changed: &mut usize) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find(source) {
            let before = &rest[..at];
            let after = &rest[at + source.len()..];
            let left_ok = before
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && !".-_/@".contains(c));
            let right_ok = after
                .chars()
                .next()
                .is_none_or(|c| "/?#".contains(c) || !c.is_alphanumeric() && !".-_:@".contains(c));
            out.push_str(before);
            if left_ok && right_ok {
                out.push_str(target);
                *changed += 1;
            } else {
                out.push_str(source);
            }
            rest = after;
        }
        out.push_str(rest);
        out
    }
    fn structured(value: &mut Value, source: &str, target: &str, changed: &mut usize) {
        match value {
            Value::String(s) => *s = rewrite(s, source, target, changed),
            Value::Array(a) => a
                .iter_mut()
                .for_each(|v| structured(v, source, target, changed)),
            Value::Object(o) => o
                .values_mut()
                .for_each(|v| structured(v, source, target, changed)),
            _ => (),
        }
    }
    let fields: &[(&str, &[&str])] = &[
        (
            "posts",
            &[
                "body",
                "published_body",
                "document",
                "published_document",
                "fields",
                "published_fields",
                "blocks",
                "published_blocks",
                "seo",
                "published_seo",
            ],
        ),
        ("revisions", &["snapshot"]),
        ("themes", &["draft", "live"]),
        ("theme_revisions", &["package"]),
        ("settings", &["navigation"]),
        ("site_design", &["draft_options", "live_options"]),
        ("redirects", &["target"]),
    ];
    for (table, columns) in fields {
        for row in snapshot.tables.get_mut(*table).unwrap() {
            for column in *columns {
                let text = row[*column].as_str().unwrap();
                let output = if let Ok(mut value) = serde_json::from_str::<Value>(text) {
                    structured(&mut value, source, &target, &mut changed);
                    serde_json::to_string(&value).unwrap()
                } else {
                    rewrite(text, source, &target, &mut changed)
                };
                row.insert((*column).into(), Value::String(output));
            }
        }
    }
    snapshot.tables.insert("recovery_mode".into(),vec![serde_json::from_value(serde_json::json!({"id":1,"held":1,"source_origin":source,"target_origin":target,"review":""})).unwrap()]);
    // Source sessions and WebAuthn credentials must not authenticate a new site.
    // Password accounts and financial identities remain stable for owner review.
    // Sessions are deliberately excluded from all full-site recovery packages.
    snapshot.tables.get_mut("user_passkeys").unwrap().clear();
    let payload =
        serde_json::to_string(&snapshot).map_err(|_| Error::invalid("Cannot serialize clone."))?;
    let bytes = serde_json::to_vec(&Envelope {
        format: "wpalt-backup-v12".into(),
        sha256: digest(payload.as_bytes()),
        payload,
    })
    .map_err(|_| Error::invalid("Cannot serialize clone."))?;
    validate(config, &bytes)?;
    let source_sha = digest(encoded);
    let output_sha = digest(&bytes);
    let plan = digest(format!("wpalt-clone-v1:{source_sha}:{output_sha}").as_bytes());
    let report = serde_json::json!({"format":"wpalt-clone-v1","plan":plan,"source_sha256":source_sha,"output_sha256":output_sha,"source_origin":source,"target_origin":target,"rewritten_occurrences":changed,"output_bytes":bytes.len(),"held":true,"boundary":"Fresh-target read-only clone; background cycles and HTTP writes other than password login/logout are blocked until stopped-host activation. Source sessions and origin-bound passkeys are removed. Presentation fields only are rewritten; queued messages, forms, external identity callbacks, analytics script files, provider identities and immutable financial history require explicit review. No live graph merge."});
    Ok(Prepared {
        report,
        bytes,
        plan,
    })
}

fn decode(text: &str) -> String {
    let mut bytes = Vec::with_capacity(text.len());
    let source = text.as_bytes();
    let mut n = 0;
    while n < source.len() {
        if source[n] == b'%'
            && n + 2 < source.len()
            && let Ok(hex) = std::str::from_utf8(&source[n + 1..n + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            bytes.push(byte);
            n += 3;
        } else {
            bytes.push(source[n]);
            n += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn paths(text: &str, index: &BTreeMap<String, BTreeSet<String>>, found: &mut HashSet<String>) {
    let decoded = decode(text);
    for token in decoded.split(|c: char| !(c.is_alphanumeric() || "/:?#%=_-.~&+@".contains(c))) {
        let path = if token.starts_with("http://") || token.starts_with("https://") {
            url::Url::parse(token).ok().map(|u| u.path().to_owned())
        } else if token.starts_with('/') && !token.starts_with("//") {
            Some(token.split(['?', '#']).next().unwrap_or("").to_owned())
        } else {
            None
        };
        if let Some(path) = path
            && let Some(ids) = index.get(path.trim_end_matches('/'))
        {
            found.extend(ids.iter().cloned());
        }
    }
    // Structured strings can escape URLs. Decode containers before token scanning.
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        fn walk(
            value: &Value,
            index: &BTreeMap<String, BTreeSet<String>>,
            found: &mut HashSet<String>,
        ) {
            match value {
                Value::String(text) => paths(text, index, found),
                Value::Array(values) => values.iter().for_each(|v| walk(v, index, found)),
                Value::Object(values) => values.iter().for_each(|(k, v)| {
                    paths(k, index, found);
                    walk(v, index, found);
                }),
                _ => (),
            }
        }
        if value.is_array() || value.is_object() {
            walk(&value, index, found);
        }
    }
}

pub fn prepare(config: &Config, encoded: &[u8], requested: &[String]) -> Result<Prepared> {
    if requested.is_empty() || requested.len() > 1000 {
        return Err(Error::invalid(
            "Select between one and 1,000 content UUIDs.",
        ));
    }
    let mut snapshot = validate(config, encoded)?;
    let discovery: crate::discovery::Definition = serde_json::from_str(
        snapshot.tables["discovery_settings"][0]["definition"]
            .as_str()
            .unwrap(),
    )
    .map_err(|_| Error::invalid("Invalid discovery settings."))?;
    let posts = &snapshot.tables["posts"];
    let ids: HashSet<String> = posts
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    let mut roots = BTreeSet::new();
    for id in requested {
        let id = uuid::Uuid::parse_str(id)
            .map_err(|_| Error::invalid("Select content UUIDs."))?
            .to_string();
        if !ids.contains(&id) {
            return Err(Error::invalid(
                "Selected content is absent from this recovery point.",
            ));
        }
        roots.insert(id);
    }
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut post_groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in posts {
        let id = row["id"].as_str().unwrap().to_owned();
        for (slug, locale, group) in [
            ("slug", "locale", "translation_group"),
            (
                "published_slug",
                "published_locale",
                "published_translation_group",
            ),
        ] {
            let slug = row[slug].as_str().unwrap();
            let locale = row[locale].as_str().unwrap();
            if !slug.is_empty() {
                index
                    .entry(format!("/{locale}/{slug}"))
                    .or_default()
                    .insert(id.clone());
                if locale == discovery.default_language {
                    index
                        .entry(format!("/{slug}"))
                        .or_default()
                        .insert(id.clone());
                }
            }
            let group = row[group].as_str().unwrap();
            if !group.is_empty() {
                groups
                    .entry(group.to_owned())
                    .or_default()
                    .insert(id.clone());
                post_groups
                    .entry(id.clone())
                    .or_default()
                    .insert(group.to_owned());
            }
        }
    }
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut required = BTreeSet::new();
    let mut scanned = 0usize;
    let mut edge_count = 0usize;
    let mut row_count = 0usize;
    for (table, rows) in &snapshot.tables {
        for row in rows {
            row_count += 1;
            if row_count > 100_000 {
                return Err(Error::invalid(
                    "Selective recovery scan exceeds 100,000 records; no package created.",
                ));
            }
            let mut found = HashSet::new();
            for value in row.values().filter_map(Value::as_str) {
                scanned = scanned.saturating_add(value.len());
                if scanned > SCAN_BYTES {
                    return Err(Error::invalid(
                        "Selective recovery scan exceeds 64 MiB; no package created.",
                    ));
                }
                crate::operations::cleanup::references(value, &mut found);
                paths(value, &index, &mut found);
                if found.len() > EDGES {
                    return Err(Error::invalid(
                        "Selective recovery reference budget exceeded.",
                    ));
                }
            }
            let found: BTreeSet<_> = found.into_iter().filter(|id| ids.contains(id)).collect();
            edge_count = edge_count.saturating_add(found.len());
            if edge_count > EDGES {
                return Err(Error::invalid(
                    "Selective recovery exceeds 100,000 reference edges; no package created.",
                ));
            }
            if table == "posts" {
                edges
                    .entry(row["id"].as_str().unwrap().to_owned())
                    .or_default()
                    .extend(found);
            } else if CHILDREN.contains(&table.as_str()) {
                edges
                    .entry(row["post_id"].as_str().unwrap().to_owned())
                    .or_default()
                    .extend(found);
            } else {
                required.extend(found);
            }
        }
    }
    let mut retained = roots.clone();
    retained.extend(required.iter().cloned());
    let mut pending: VecDeque<_> = retained.iter().cloned().collect();
    while let Some(id) = pending.pop_front() {
        let mut dependencies = edges.get(&id).cloned().unwrap_or_default();
        for group in post_groups.get(&id).into_iter().flatten() {
            dependencies.extend(groups[group].iter().cloned());
        }
        for dependency in dependencies {
            if retained.insert(dependency.clone()) {
                pending.push_back(dependency);
            }
        }
    }
    let omitted: BTreeSet<_> = ids
        .iter()
        .filter(|id| !retained.contains(*id))
        .cloned()
        .collect();
    snapshot
        .tables
        .get_mut("posts")
        .unwrap()
        .retain(|r| retained.contains(r["id"].as_str().unwrap()));
    for table in CHILDREN {
        snapshot
            .tables
            .get_mut(*table)
            .unwrap()
            .retain(|r| retained.contains(r["post_id"].as_str().unwrap()));
    }
    let payload = serde_json::to_string(&snapshot)
        .map_err(|_| Error::invalid("Cannot serialize selected recovery graph."))?;
    let bytes = serde_json::to_vec(&Envelope {
        format: "wpalt-backup-v12".into(),
        sha256: digest(payload.as_bytes()),
        payload,
    })
    .map_err(|_| Error::invalid("Cannot serialize selected recovery graph."))?;
    // Revalidate every shared module before making an output file available.
    validate(config, &bytes)?;
    let source_sha = digest(encoded);
    let output_sha = digest(&bytes);
    let plan = digest(format!("wpalt-editorial-selection-v1:{source_sha}:{output_sha}").as_bytes());
    let report = serde_json::json!({"format":"wpalt-editorial-selection-v1","plan":plan,"source_sha256":source_sha,"output_sha256":output_sha,"output_bytes":bytes.len(),"requested_posts":roots,"retained_posts":retained,"retained_for_shared_domains":required,"omitted_posts":omitted,"tables":snapshot.tables.iter().map(|(k,v)|(k.clone(),v.len())).collect::<BTreeMap<_,_>>(),"scan_bytes":scanned,"reference_edges":edge_count,"boundary":"Editorial selection only. All users, protected-resource rules, learning, financial, workflow, media and private attachment records remain. Conservative UUID/URL and translation dependencies can retain extra content. Restore only into an empty fresh target; ordinary recovery side-effect and provider-ownership precautions still apply."});
    Ok(Prepared {
        report,
        bytes,
        plan,
    })
}
