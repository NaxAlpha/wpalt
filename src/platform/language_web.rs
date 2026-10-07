//! Connected manual language workflows; private variants remain draft/review authority.
use super::translations;
use crate::{
    App, auth, content, discovery,
    error::{Error, Result},
    model::{Post, Session},
    schema, view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Form, Path, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use maud::{Markup, html};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use std::collections::BTreeMap;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/languages", get(index))
        .route("/admin/languages/{id}", get(detail).post(prepare))
        .route("/api/admin/translations/{id}", post(api))
        .layer(DefaultBodyLimit::max(8192))
}
async fn editor(app: &App, headers: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, headers).await?;
    if !s.can_edit() || s.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    Ok(s)
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    after: Option<String>,
    #[serde(default)]
    locale: String,
}
async fn index(
    State(app): State<App>,
    headers: HeaderMap,
    Query(filter): Query<Filter>,
) -> Result<Html<String>> {
    let s = editor(&app, &headers).await?;
    let d = discovery::load(&app).await?.0;
    if !filter.locale.is_empty() {
        d.language(&filter.locale)?;
    }
    let mut q = sqlx::QueryBuilder::<sqlx::Any>::new(
        "SELECT id,title,locale,translation_group,status,updated_at FROM posts WHERE 1=1",
    );
    if !filter.locale.is_empty() {
        q.push(" AND locale=").push_bind(&filter.locale);
    }
    if let Some(after) = &filter.after {
        let (time, id) = after
            .split_once(':')
            .ok_or_else(|| Error::invalid("Invalid language cursor."))?;
        let time: i64 = time
            .parse()
            .map_err(|_| Error::invalid("Invalid language cursor."))?;
        if time < 0 || uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::invalid("Invalid language cursor."));
        }
        q.push(" AND (updated_at,id)<(")
            .push_bind(time)
            .push(",")
            .push_bind(id)
            .push(")");
    }
    q.push(" ORDER BY updated_at DESC,id DESC LIMIT 41");
    let rows = app.db.fetch_builder(&mut q).await?;
    let body = html! {(view::heading("Publishing","Language workspace","Compare language variants and review deliberate draft duplication or shared-value synchronization."))
    form method="get" class="toolbar" {label {"Content language" select name="locale" {option value="" {"All languages"}@for l in &d.languages{option value=(l.code) selected[filter.locale==l.code] {(l.label)}}}}button class="secondary" {"Show content"}}
    section class="panel" {@if rows.is_empty(){p {"No content in this language."}}@for r in rows.iter().take(40){article {h2 {a href=(format!("/admin/languages/{}",r.get::<String,_>("id"))) {(r.get::<String,_>("title"))}}p {bdi {(r.get::<String,_>("locale"))} " · " (r.get::<String,_>("status"))}p class="muted" {(if r.get::<String,_>("translation_group").is_empty(){"Save a translation group in the editor to connect variants."}else{"Open to compare linked variants."})}}}}
    @if rows.len()>40{@let r=&rows[39];a class="button secondary" href=(format!("/admin/languages?locale={}&after={}:{}",filter.locale,r.get::<i64,_>("updated_at"),r.get::<String,_>("id"))) {"Older content →"}}
    };
    Ok(Html(view::layout(
        "Language workspace",
        &app.db.settings().await?,
        Some(&s),
        body,
    )))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    #[serde(default)]
    locale: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    slug: String,
}
async fn detail(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(selection): Query<Selection>,
) -> Result<Html<String>> {
    let s = editor(&app, &headers).await?;
    render(&app, &s, &id, selection, &[], None).await
}
async fn render(
    app: &App,
    s: &Session,
    id: &str,
    selection: Selection,
    fields: &[String],
    report: Option<&Value>,
) -> Result<Html<String>> {
    if uuid::Uuid::parse_str(id).is_err()
        || (!selection.target.is_empty() && uuid::Uuid::parse_str(&selection.target).is_err())
    {
        return Err(Error::invalid("Choose a valid language variant."));
    }
    let source = content::get(app, id).await?;
    let d = discovery::load(app).await?.0;
    let registry = schema::Registry::load(app).await?;
    let variants = if source.translation_group.is_empty() {
        vec![]
    } else {
        sqlx::query("SELECT id,title,locale,status,version FROM posts WHERE translation_group=$1 ORDER BY locale,id LIMIT 16").bind(&source.translation_group).fetch_all(&app.db.pool).await?
    };
    let target = if selection.target.is_empty() {
        None
    } else {
        let target = content::get(app, &selection.target).await?;
        if target.kind != source.kind
            || target.translation_group != source.translation_group
            || source.translation_group.is_empty()
        {
            return Err(Error::invalid("Choose a linked language variant."));
        }
        Some(target)
    };
    let locale = if selection.locale.is_empty() {
        target
            .as_ref()
            .map(|p| p.locale.clone())
            .or_else(|| {
                d.languages
                    .iter()
                    .find(|l| l.code != source.locale)
                    .map(|l| l.code.clone())
            })
            .unwrap_or_default()
    } else {
        selection.locale
    };
    let slug = if selection.slug.is_empty() {
        target.as_ref().map(|p| p.slug.clone()).unwrap_or_default()
    } else {
        selection.slug
    };
    let copy = |p: &Post| -> Markup {
        let language = d.language(&p.locale).ok();
        let copied = p.id != source.id && p.document == source.document && p.title == source.title;
        let text_locale = if copied { &source.locale } else { &p.locale };
        let text_direction = d
            .language(text_locale)
            .map(|l| l.direction.as_str())
            .unwrap_or("ltr");
        let text = crate::document::Document::parse(&p.document)
            .map(|d| d.markdown())
            .unwrap_or_else(|_| p.body.clone());
        let long = text.chars().count() > 600;
        html! {section class="panel" lang=(p.locale) dir=(language.map(|l|l.direction.as_str()).unwrap_or("ltr")){
            h2 {(language.map(|l|l.label.as_str()).unwrap_or(p.locale.as_str()))}
            p lang="en" dir="ltr" {bdi {(p.status)} " · " (p.version)}
            h3 lang=(text_locale) dir=(text_direction) {(p.title)}
            @if copied {p class="notice" lang="en" dir="ltr" {"This draft still contains source-language text. Translate and review it before publishing."}}
            div lang=(text_locale) dir=(text_direction) {
                @if long {pre class="translation-copy" {(text.chars().take(320).collect::<String>()) "…"}
                    details {summary lang="en" dir="ltr" {"Read complete saved document"}pre class="translation-copy" {(text)}}
                } @else {pre class="translation-copy" {(text)}}
            }
            a class="button secondary" lang="en" dir="ltr" href=(format!("/admin/posts/{}",p.id)){"Edit this variant"}
        }}
    };
    let body = html! {
    (view::heading("Publishing","Compare language variants","Each language publishes and earns editorial approval independently. Compare saved content before creating or updating a draft."))
    p {a href="/admin/languages" {"All language content"}}
    nav aria-label="Related language variants" {@for r in variants.iter().filter(|r|r.get::<String,_>("id")!=source.id){a class="button secondary" href=(format!("/admin/languages/{id}?target={}",r.get::<String,_>("id"))){(r.get::<String,_>("locale")) " · " (r.get::<String,_>("status"))}}}
    div class="split" {(copy(&source))@if let Some(target)=&target{(copy(target))}@else{section class="panel" {h2 {"New translation draft"}p {"The source document is copied without automatic translation. Edit its language before requesting review or publication. Protected source duplication requires an explicitly provisioned protected target."}}}}
    @if source.translation_group.is_empty(){p class="notice" {"Assign a translation group in the source editor and save before connecting a different language."}}
    @else if !d.languages.iter().any(|l|l.code!=source.locale){p class="notice" {"Configure another site language before creating a translation."}}
    @else{form method="post" action=(format!("/admin/languages/{id}")) class="panel" {(view::csrf(s))input type="hidden" name="target" value=(selection.target);h2 {(if target.is_some(){"Review shared-value synchronization"}else{"Review a translation draft"})}
    label {"Target language" select name="locale" aria-label="Target language" {@for l in d.languages.iter().filter(|l|l.code!=source.locale){option value=(l.code) selected[l.code==locale] {(l.label)}}}}
    label {"Target URL slug" input name="slug" aria-label="Target URL slug" value=(slug) pattern="[a-z0-9-]+" maxlength="120" required;}
    @if target.is_some(){fieldset {legend {"Shared values to copy"}p {"Only checked values are copied. Translated document, title, URL, discovery, classification, publication and access remain independently authored."}@for (name,_) in registry.fields_for(&source.kind)?{label {input type="checkbox" name=(format!("sync_{name}")) value="true" checked[fields.contains(&name)];(name)}}}}
    @if let Some(report)=report{p class="notice" {"Review this exact saved source, target and selection. Changes before execution require a new preview."}p {"Source revision " (report["source_version"].as_i64().unwrap_or(0))}input type="hidden" name="execute" value=(report["plan"].as_str().unwrap_or(""));button {(if target.is_some(){"Apply reviewed shared values"}else{"Create reviewed draft"})}}
    @else{button {"Preview language operation"}}
    }}
    };
    Ok(Html(view::layout(
        "Compare language variants",
        &app.db.settings().await?,
        Some(s),
        body,
    )))
}
async fn prepare(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(mut form): Form<BTreeMap<String, String>>,
) -> Result<Response> {
    let s = editor(&app, &headers).await?;
    auth::csrf(&s, &form.remove("csrf").unwrap_or_default())?;
    let selection = Selection {
        locale: form.remove("locale").unwrap_or_default(),
        target: form.remove("target").unwrap_or_default(),
        slug: form.remove("slug").unwrap_or_default(),
    };
    let execute = form.remove("execute");
    let mut fields = Vec::new();
    for (key, value) in form {
        if let Some(name) = key.strip_prefix("sync_") {
            if value != "true" || !schema::identifier(name) {
                return Err(Error::invalid("Choose declared shared values."));
            }
            fields.push(name.to_owned());
        } else {
            return Err(Error::invalid("Unknown language operation value."));
        }
    }
    let report = translations::prepare_as(
        &app,
        &s,
        translations::Request {
            source: &id,
            locale: &selection.locale,
            slug: &selection.slug,
            target: (!selection.target.is_empty()).then_some(selection.target.as_str()),
            fields: &fields,
            execute: execute.as_deref(),
        },
    )
    .await?;
    if report["executed"] == true {
        let result = report["result"]["id"]
            .as_str()
            .ok_or_else(|| Error::invalid("Missing language draft receipt."))?;
        Ok(Redirect::to(&format!("/admin/posts/{result}")).into_response())
    } else {
        Ok(render(&app, &s, &id, selection, &fields, Some(&report))
            .await?
            .into_response())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    csrf: String,
    locale: String,
    slug: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    execute: Option<String>,
}
async fn api(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Operation>,
) -> Result<Json<Value>> {
    let s = editor(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    Ok(Json(
        translations::prepare_as(
            &app,
            &s,
            translations::Request {
                source: &id,
                locale: &input.locale,
                slug: &input.slug,
                target: input.target.as_deref(),
                fields: &input.fields,
                execute: input.execute.as_deref(),
            },
        )
        .await?,
    ))
}
