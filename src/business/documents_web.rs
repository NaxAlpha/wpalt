//! Private owner generation from canonical frozen responses, never client source data.
use super::{documents, forms::Widget, store::PublishedForm, web::editor_session};
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Router,
    extract::{Form, Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::post,
};
use maud::html;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

pub fn routes() -> Router<App> {
    Router::new().route("/admin/forms/{id}/entries/{entry}/document", post(generate))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Generate {
    csrf: String,
    kind: String,
}

pub fn controls(
    app: &App,
    session: &crate::model::Session,
    form: &str,
    entry: &str,
) -> maud::Markup {
    html! {section class="panel" {h2 {"Response document"}
        p {"A server attestation preserves the exact document and original acknowledgment. Respondent identity remains unverified."}
        @if let Some(identity)=app.documents.identity() {
            p class="entry-text" {"Public-key fingerprint: " code {(identity)}}
            form method="post" data-attested-document=(identity) data-document-form=(form) data-document-entry=(entry) action=(format!("/admin/forms/{form}/entries/{entry}/document")) {
                (view::csrf(session))
                label for="response-document-format" {"Document format"}
                select id="response-document-format" name="kind" {option value="html" {"Printable HTML and verification bundle"}
                    @if app.config.documents.pdf_enabled {option value="pdf" {"PDF and verification bundle"}}
                }
                div class="toolbar" {button {"Prepare document"}}
                p data-document-status role="status" hidden {}
                div class="toolbar" data-document-downloads hidden {}
            }
            p {"Keep the verification bundle with the document. For independent verification, obtain this fingerprint through a trusted channel and use document-verify. Without JavaScript, the download is the bundle; the command extracts its artifact."}
        } @else {p {"Signing is disabled. The operator can create an owner-controlled document key and enable the documents configuration."}}
        p {(if app.config.attachment_scanning.enabled {"Private attachment downloads must pass the configured local scanner."}else{"Malware scanning is disabled for private attachments."})}
    }}
}

async fn generate(
    State(app): State<App>,
    headers: HeaderMap,
    Path((form, entry)): Path<(String, String)>,
    Form(input): Form<Generate>,
) -> Result<Response> {
    let session = editor_session(&app, &headers).await?;
    auth::csrf(&session, &input.csrf)?;
    if !["html", "pdf"].contains(&input.kind.as_str()) {
        return Err(Error::invalid(
            "Choose HTML or enabled PDF document output.",
        ));
    }
    if input.kind == "pdf" && !app.config.documents.pdf_enabled {
        return Err(Error::invalid("PDF document rendering is disabled."));
    }
    if app.documents.identity().is_none() {
        return Err(Error::invalid("Document signing is disabled."));
    }
    let _permit = app
        .document_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::invalid("Document generation is busy; retry shortly."))?;
    let started = std::time::Instant::now();
    let source = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        snapshot(&app, &form, &entry),
    )
    .await
    .map_err(|_| Error::invalid("Document source exceeded its ten-second deadline."))??;
    let content = printable(&source)?;
    let (artifact, media_type) = if input.kind == "pdf" {
        (
            super::document_pdf::render(&app, content.as_bytes()).await?,
            "application/pdf",
        )
    } else {
        (content.into_bytes(), "text/html")
    };
    let bundle = app.documents.attest(source, media_type, &artifact)?;
    let bytes = serde_json::to_vec(&bundle)
        .map_err(|_| Error::invalid("Cannot encode the attested document bundle."))?;
    editor_session(&app, &headers).await?;
    tracing::debug!(
        event = "response_document_generated",
        artifact_bytes = artifact.len(),
        duration_us = started.elapsed().as_micros() as u64,
        media_type
    );
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=response-document.json",
            ),
            (header::CACHE_CONTROL, "private, no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}

async fn snapshot(app: &App, form: &str, entry: &str) -> Result<Value> {
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let row=sqlx::query("SELECT e.values_json,e.created_at,e.form_version,p.definition FROM form_entries e JOIN form_publications p ON p.form_id=e.form_id AND p.version=e.form_version WHERE e.form_id=$1 AND e.id=$2").bind(form).bind(entry).fetch_optional(&mut *tx).await?.ok_or_else(Error::not_found)?;
    let raw: String = row.get("definition");
    let accepted: String = row.get("values_json");
    if raw.len() > 128 * 1024 || accepted.len() > 128 * 1024 {
        return Err(Error::invalid("Document source exceeds its byte limit."));
    }
    let publication: PublishedForm = serde_json::from_str(&raw)
        .map_err(|_| Error::invalid("Stored publication needs repair."))?;
    let values: Value = serde_json::from_str(&accepted)
        .map_err(|_| Error::invalid("Stored response needs repair."))?;
    if publication
        .form
        .evaluate(&publication.common(), &values, false)?
        != values
    {
        return Err(Error::invalid(
            "A stored response is not canonical; repair it before generating a document.",
        ));
    }
    let rows=sqlx::query("SELECT id,field_name,original_name,mime,size,sha256,filename FROM form_attachments WHERE form_id=$1 AND entry_id=$2 ORDER BY id LIMIT 33").bind(form).bind(entry).fetch_all(&mut *tx).await?;
    if rows.len() > 32 {
        return Err(Error::invalid(
            "Document attachment inventory exceeds its limit.",
        ));
    }
    let mut attachments = Vec::new();
    for row in rows {
        let filename: String = row.get("filename");
        let size: i64 = row.get("size");
        let digest: String = row.get("sha256");
        let id: String = row.get("id");
        let field: String = row.get("field_name");
        if !super::attachments::safe_filename(&filename)
            || !(1..=2 * 1024 * 1024).contains(&size)
            || values.get(&field).and_then(Value::as_str) != Some(id.as_str())
        {
            return Err(Error::invalid(
                "Document attachment inventory needs repair.",
            ));
        }
        let bytes = crate::backup::read_bounded(
            &app.config.data_dir.join("attachments").join(filename),
            2 * 1024 * 1024,
        )
        .await?;
        if bytes.len() as i64 != size || auth::digest(&bytes) != digest {
            return Err(Error::invalid(
                "Document attachment integrity check failed.",
            ));
        }
        attachments.push(json!({"id":id,"field":field,"name":row.get::<String,_>("original_name"),"mime":row.get::<String,_>("mime"),"bytes":size,"sha256":digest}));
    }
    for field in &publication.form.fields {
        if matches!(field.widget, Some(Widget::Upload))
            && values.get(&field.name).is_some()
            && !attachments.iter().any(|file| file["field"] == field.name)
        {
            return Err(Error::invalid(
                "Document source is missing an accepted attachment.",
            ));
        }
    }
    let fields:Vec<_>=publication.form.fields.iter().filter_map(|field|values.get(&field.name).map(|value| {
        let statement=match &field.widget {Some(Widget::Signature{statement}|Widget::Acknowledgment{statement})=>Some(statement.as_str()),_=>None};
        json!({"name":field.name,"label":if field.schema.label.is_empty(){field.name.as_str()}else{field.schema.label.as_str()},"value":value,"statement":statement})
    })).collect();
    let source = json!({"form_id":form,"entry_id":entry,"form_version":row.get::<i64,_>("form_version"),"accepted_at":row.get::<i64,_>("created_at"),"title":publication.form.title,"definition_sha256":auth::digest(raw.as_bytes()),"fields":fields,"attachments":attachments});
    tx.commit().await?;
    Ok(source)
}

pub fn printable(source: &Value) -> Result<String> {
    let fields = source["fields"]
        .as_array()
        .ok_or_else(|| Error::invalid("Invalid document source."))?;
    let content=html! {(maud::DOCTYPE)html lang="en" {head {meta charset="utf-8";
        meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'";
        meta name="viewport" content="width=device-width,initial-scale=1";
        title {(source["title"].as_str().unwrap_or("Response document"))}
        style {"body{font:16px/1.6 system-ui,sans-serif;margin:48px;color:#263034}main{max-width:820px;margin:auto}h1{font-size:30px;line-height:1.2}section{border-top:1px solid #dadeda;padding:20px 0}p,pre{overflow-wrap:anywhere;white-space:pre-wrap}pre{font:inherit}small{color:#46545a}@page{margin:20mm}"}
    }body {main {h1 {(source["title"].as_str().unwrap_or("Response document"))}
        p {"Response " (source["entry_id"].as_str().unwrap_or("")) " · published version " (source["form_version"].as_i64().unwrap_or(0))}
        @if let Some(time)=source["accepted_at"].as_i64().and_then(|value|chrono::DateTime::from_timestamp(value,0)) {p {"Accepted " (time.format("%Y-%m-%d %H:%M:%S UTC"))}}
        p {"Server attestation only. Respondent identity is unverified."}
        @for field in fields {section {h2 {(field["label"].as_str().unwrap_or("Field"))}
            @if let Some(statement)=field["statement"].as_str() {p {(statement)}}
            @if field["statement"].is_string() && field["value"].get("name").and_then(Value::as_str).is_some() {p {"Declared name: " (field["value"]["name"].as_str().unwrap_or(""))}p {"Accepted this statement."}}
            @else if let Some(attachment)=source["attachments"].as_array().and_then(|items|items.iter().find(|item|item["field"]==field["name"])) {p {(attachment["name"].as_str().unwrap_or("Private attachment"))}p {"Private attachment · " (attachment["bytes"].as_u64().unwrap_or(0)) " bytes. Its digest is retained in the verification bundle."}}
            @else if let Some(text)=field["value"].as_str() {p dir="auto" {(text)}} @else if let Some(value)=field["value"].as_bool() {p {(if value {"Yes"}else{"No"})}} @else {pre {(serde_json::to_string_pretty(&field["value"]).map_err(|_|Error::invalid("Invalid document value."))?)}}
        }}
        small {"The accompanying signed manifest binds this exact artifact, frozen publication, acknowledgment and private attachment digests. Verify using independent public-key trust."}
    }}}}.into_string();
    if content.len() > documents::MAX_ARTIFACT {
        return Err(Error::invalid("Printable document exceeds its byte limit."));
    }
    Ok(content)
}
