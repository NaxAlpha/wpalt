//! A native owner workspace. Preview and HTTP fallback use the same route plan.
use super::{
    admin_session,
    redirects::{self, Plan, Rule, Settings},
};
use crate::{
    App,
    error::{Error, Result},
    model::Session,
    view,
};
use axum::{
    Json, Router,
    extract::{Form, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use maud::{Markup, html};
use serde::Deserialize;
use sqlx::Row;

const WORKSPACE: &str = "/admin/discovery/routes";
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Browse {
    #[serde(default)]
    edit: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    after: String,
}
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleForm {
    csrf: String,
    version: i64,
    #[serde(default)]
    action: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    pattern: String,
    #[serde(default)]
    target: String,
    #[serde(default = "permanent")]
    code: i64,
    #[serde(default)]
    priority: i64,
    #[serde(default)]
    case_insensitive: Option<String>,
    #[serde(default)]
    enabled: Option<String>,
    #[serde(default)]
    sample: String,
}
fn permanent() -> i64 {
    301
}
fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
impl RuleForm {
    fn rule(&self) -> Rule {
        Rule {
            id: self.id.clone(),
            label: self.label.clone(),
            pattern: self.pattern.clone(),
            target: self.target.clone(),
            code: self.code,
            priority: self.priority,
            case_insensitive: self.case_insensitive.is_some(),
            enabled: self.enabled.is_some(),
        }
    }
    fn from_rule(rule: &Rule, version: i64) -> Self {
        Self {
            version,
            id: rule.id.clone(),
            label: rule.label.clone(),
            pattern: rule.pattern.clone(),
            target: rule.target.clone(),
            code: rule.code,
            priority: rule.priority,
            case_insensitive: rule.case_insensitive.then(|| "on".into()),
            enabled: rule.enabled.then(|| "on".into()),
            ..Self::default()
        }
    }
}
#[derive(Default)]
struct Feedback {
    settings: Option<SettingsForm>,
    exact: Option<super::RedirectForm>,
    message: Option<&'static str>,
    preview: Option<Markup>,
}
fn rule_fields(session: &Session, f: &RuleForm) -> Markup {
    html! { form method="post" action="/admin/discovery/routes/rule" {
        (view::csrf(session)) input type="hidden" name="version" value=(f.version);
        div class="field-row" {
            label {"Rule identifier" input name="id" required maxlength="48" value=(f.id) placeholder="archive_year";}
            label {"Label" input name="label" required maxlength="100" value=(f.label) placeholder="Old yearly archives";}
        }
        label {"Whole-path pattern" input name="pattern" required maxlength="512" value=(f.pattern) placeholder="^/old/([a-z0-9-]+)$";}
        label {"Local destination" input name="target" required maxlength="300" value=(f.target) placeholder="/articles/$1";}
        p class="muted" {"Use ^/…$ and numbered captures such as $1. Queries are discarded. Lookaround and backreferences are unavailable; every expanded destination must be a safe local path."}
        div class="field-row" {
            label {"HTTP status" select name="code" {option value="301" selected[f.code==301] {"301 · permanent"} option value="302" selected[f.code==302] {"302 · temporary"}}}
            label {"Priority · lower runs first" input type="number" name="priority" min="0" max="10000" value=(f.priority);}
        }
        label {input type="checkbox" name="case_insensitive" checked[f.case_insensitive.is_some()]; " Ignore letter case"}
        label {input type="checkbox" name="enabled" checked[f.enabled.is_some()]; " Enabled"}
        label {"Sample path" input name="sample" maxlength="300" value=(f.sample) placeholder="/old/garden";}
        p class="muted" {"Preview temporarily includes this proposed rule. It does not save. Samples reveal selected overlaps and loops; they cannot prove that all patterns are disjoint."}
        div class="toolbar" {button class="secondary" name="action" value="preview" {"Preview rule"} button name="action" value="save" {"Save rule"}}
    }}
}
async fn render(
    app: &App,
    session: &Session,
    browse: &Browse,
    proposed: Option<&RuleForm>,
    feedback: Feedback,
) -> Result<Response> {
    let (policy, version) = redirects::load(app).await?;
    let exact =
        sqlx::query("SELECT source,target,code,version FROM redirects ORDER BY source LIMIT 1000")
            .fetch_all(&app.db.pool)
            .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM missing_paths")
        .fetch_one(&app.db.pool)
        .await?;
    let missing=sqlx::query("SELECT path,hits,first_seen,last_seen,version FROM missing_paths WHERE path>$1 ORDER BY path LIMIT 101")
        .bind(&browse.after).fetch_all(&app.db.pool).await?;
    let more = missing.len() > 100;
    let chosen = policy.settings.rules.iter().find(|r| r.id == browse.edit);
    let initial = chosen
        .map(|r| RuleForm::from_rule(r, version))
        .unwrap_or(RuleForm {
            version,
            code: 301,
            enabled: Some("on".into()),
            ..RuleForm::default()
        });
    let fields = proposed.unwrap_or(&initial);
    let settings = feedback.settings.as_ref();
    let settings_version = settings.map_or(version, |f| f.version);
    let automatic = settings.map_or(policy.settings.automatic_slug_redirects, |f| {
        f.automatic_slug_redirects.is_some()
    });
    let reporting = settings.map_or(policy.settings.report_404, |f| f.report_404.is_some());
    let retention = settings.map_or(policy.settings.retention_days, |f| f.retention_days);
    let maximum = settings.map_or(policy.settings.max_paths, |f| f.max_paths);
    let exact_form = feedback.exact.as_ref();
    let exact_source = exact_form.map_or(browse.source.as_str(), |f| f.source.as_str());
    let exact_target = exact_form.map_or("", |f| f.target.as_str());
    let exact_code = exact_form.map_or(301, |f| f.code);
    let exact_version = exact_form.map_or(0, |f| f.version);
    let body = html! {
        (view::heading("Discovery", "Redirects & missing links", "Keep published addresses working, preview route changes and investigate missing public paths."))
        p {a href="/admin/discovery" {"Discovery settings"} " · " a href="/admin/discovery/links" {"Published link checks"} " · " a href="/admin/discovery/rendered-links" {"Rendered link graph"}}
        @if let Some(message)=feedback.message {div class="notice error" role="alert" {(message)}}
        @if let Some(preview)=feedback.preview {section class="panel" aria-label="Rule preview" {h2 {"Preview"}(preview)}}
        section class="panel" {h2 {"Publication routes & reporting"}
            form method="post" action="/admin/discovery/routes/settings" {
                (view::csrf(session))input type="hidden" name="version" value=(settings_version);
                label {input type="checkbox" name="automatic_slug_redirects" checked[automatic]; " Create redirects when a previously published address changes"}
                p class="muted" {"Working-copy edits create no redirects. Publication updates owned aliases together; manual rules are never overwritten. Turning this off stops new aliases; remove retained aliases below when needed."}
                label {input type="checkbox" name="report_404" checked[reporting]; " Retain missing public paths"}
                p class="muted" {"Off by default. Retains safe public path counts and times, excluding queries, client addresses and private or reserved routes. Slugs can contain personal information. Disabling clears the report."}
                div class="field-row" {label {"Retention · days" input type="number" name="retention_days" min="1" max="90" value=(retention);}label {"Maximum retained paths" input type="number" name="max_paths" min="1" max="2000" value=(maximum);}}
                button {"Save route settings"}
            }
        }
        section class="panel" {h2 {"Exact redirects"}
            p class="muted" {"Exact manual rules win, then owned publication aliases, then patterns ordered by priority and identifier. Existing public routes retain authority."}
            form class="field-row" method="post" action="/admin/discovery/redirects" {
                (view::csrf(session))input type="hidden" name="return_workspace" value="on";input type="hidden" name="version" value=(exact_version);
                label {"Source path" input name="source" required maxlength="300" value=(exact_source) placeholder="/old-page";}
                label {"Destination path" input name="target" required maxlength="300" value=(exact_target) placeholder="/new-page";}
                label {"HTTP status" select name="code" {option value="301" selected[exact_code==301] {"301 · permanent"}option value="302" selected[exact_code==302] {"302 · temporary"}}}
                button {"Add redirect"}
            }
            @if exact.is_empty() {p class="muted" {"No exact redirects."}}
            @for r in &exact {form class="toolbar" method="post" action="/admin/discovery/redirects" {
                (view::csrf(session))input type="hidden" name="return_workspace" value="on";
                input type="hidden" name="source" value=(r.get::<String,_>("source")); input type="hidden" name="target" value=(r.get::<String,_>("target"));
                input type="hidden" name="code" value=(r.get::<i64,_>("code"));input type="hidden" name="version" value=(r.get::<i64,_>("version"));
                span {(r.get::<String,_>("source"))" → "(r.get::<String,_>("target"))}
                button class="secondary" name="action" value="delete" {"Remove exact rule"}
            }}
        }
        section class="panel" {h2 {"Capture rules"}
            @if policy.settings.rules.is_empty() {p class="muted" {"No capture rules. Add a named pattern and preview a real path before saving."}}
            @for rule in &policy.settings.rules {div class="toolbar" {
                a href=(format!("{WORKSPACE}?edit={}",rule.id)) {(rule.label)}
                span class="muted" {(rule.priority)" · "(if rule.enabled {"enabled"}else{"disabled"})}
                form method="post" action="/admin/discovery/routes/rule" {
                    (view::csrf(session))input type="hidden" name="version" value=(version);input type="hidden" name="id" value=(rule.id);
                    button class="secondary" name="action" value="remove" {"Remove capture rule"}
                }
            }}
            details open[proposed.is_some() || chosen.is_some() || policy.settings.rules.is_empty()] {summary {"Add or edit a capture rule"}(rule_fields(session, fields))}
        }
        section class="panel" {h2 {"Owned publication aliases"}
            @if policy.aliases.is_empty() {p class="muted" {"No publication aliases."}}
            @for (source,alias) in &policy.aliases {form class="toolbar" method="post" action="/admin/discovery/routes/report" {
                (view::csrf(session))input type="hidden" name="version" value=(version);input type="hidden" name="path" value=(source);
                span {(source)" → "(alias.target)}a href=(format!("/admin/posts/{}",alias.post_id)) {"Edit publication"}
                button class="secondary" name="action" value="alias" {"Remove alias"}
            }}
        }
        section class="panel" {h2 {"Missing public paths"}
            p class="muted" {(count)" retained · "(if policy.settings.report_404 {"reporting enabled"}else{"reporting disabled"})". Counts are admitted GET responses, not unique people. New paths are skipped when the limit is full. Jobs and incoming reports enforce retention."}
            @if count>0 {form method="post" action="/admin/discovery/routes/report" {(view::csrf(session))input type="hidden" name="version" value=(version);button class="secondary" name="action" value="purge" {"Purge retained report"}}}
            @if missing.is_empty() {p {"No retained paths in this page."}}
            @for row in missing.iter().take(100) {div class="toolbar" {
                span {(row.get::<String,_>("path"))" · "(row.get::<i64,_>("hits"))" hits"}
                span class="muted" {"First: "(crate::view::timestamp(row.get("first_seen")))" · Last: "(crate::view::timestamp(row.get("last_seen")))}
                a href=(format!("{WORKSPACE}?source={}",encode(&row.get::<String,_>("path")))) {"Create exact rule"}
                form method="post" action="/admin/discovery/routes/report" {(view::csrf(session))input type="hidden" name="path" value=(row.get::<String,_>("path"));input type="hidden" name="version" value=(row.get::<i64,_>("version"));button class="secondary" name="action" value="delete" {"Delete path"}}
            }}
            @if more {a href=(format!("{WORKSPACE}?after={}",encode(&missing[99].get::<String,_>("path")))) {"Next 100 paths"}}
        }
    };
    Ok(axum::response::Html(view::layout(
        "Redirects & missing links",
        &app.db.settings().await?,
        Some(session),
        body,
    ))
    .into_response())
}
async fn page(
    State(app): State<App>,
    headers: HeaderMap,
    Query(browse): Query<Browse>,
) -> Result<Response> {
    let session = admin_session(&app, &headers).await?;
    if browse.after.len() > 200
        || browse.edit.len() > 48
        || (!browse.source.is_empty() && !redirects::safe_path(&browse.source))
    {
        return Err(Error::invalid("Invalid workspace selection."));
    }
    render(&app, &session, &browse, None, Feedback::default()).await
}
async fn rule_action(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<RuleForm>,
) -> Result<Response> {
    let session = admin_session(&app, &headers).await?;
    crate::auth::csrf(&session, &f.csrf)?;
    let result:Result<Option<Markup>>=async {
        let (mut policy,current)=redirects::load(&app).await?;
        if f.version!=current {return Err(Error::conflict());}
        let existed=policy.settings.rules.iter().any(|r|r.id==f.id);
        policy.settings.rules.retain(|r|r.id!=f.id);
        if f.action=="remove" {if !existed {return Err(Error::conflict());}}
        else if ["save","preview"].contains(&f.action.as_str()) {policy.settings.rules.push(f.rule());}
        else {return Err(Error::invalid("Choose preview, save or remove."));}
        if f.action=="preview" {
            let _read=app.mutations.lock().await;
            let mut tx=app.db.pool.begin().await?;
            let plan=Plan::new(policy,redirects::exact(&mut tx).await?)?;tx.commit().await?;
            let outcome=redirects::resolve_checked(&app,&plan,&f.sample).await?;
            return Ok(Some(html! {
                @if let Some(resolved)=outcome {
                    p {(resolved.source)" → "(resolved.destination)" · HTTP "(resolved.code)}
                    @for hop in &resolved.hops {p {(hop.rule)" · "(hop.source)" → "(hop.target)}
                        @if hop.matching_patterns.len()>1 {p class="notice" {"Selected overlap: "(hop.matching_patterns.join(", "))". Precedence chooses the first eligible rule."}}
                    }
                    p class="muted" {"A redirect destination can still be missing. Publishing and public route authority take precedence."}
                } @else {p {"No redirect: this path is served, protected, or does not match an enabled rule."}}
            }));
        }
        redirects::configure(&app,policy.settings,current).await?;Ok(None)
    }.await;
    match result {
        Ok(Some(preview)) => {
            render(
                &app,
                &session,
                &Browse::default(),
                Some(&f),
                Feedback {
                    preview: Some(preview),
                    ..Feedback::default()
                },
            )
            .await
        }
        Ok(None) => Ok(axum::response::Redirect::to(WORKSPACE).into_response()),
        Err(error) => {
            let status = error.0;
            let mut response = render(
                &app,
                &session,
                &Browse::default(),
                Some(&f),
                Feedback {
                    message: Some(error.1),
                    ..Feedback::default()
                },
            )
            .await?;
            *response.status_mut() = status;
            Ok(response)
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsForm {
    csrf: String,
    version: i64,
    #[serde(default)]
    automatic_slug_redirects: Option<String>,
    #[serde(default)]
    report_404: Option<String>,
    retention_days: i64,
    max_paths: usize,
}
async fn settings_action(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<SettingsForm>,
) -> Result<Response> {
    let session = admin_session(&app, &headers).await?;
    crate::auth::csrf(&session, &f.csrf)?;
    let (mut policy, _) = redirects::load(&app).await?;
    policy.settings.automatic_slug_redirects = f.automatic_slug_redirects.is_some();
    policy.settings.report_404 = f.report_404.is_some();
    policy.settings.retention_days = f.retention_days;
    policy.settings.max_paths = f.max_paths;
    match redirects::configure(&app, policy.settings, f.version).await {
        Ok(_) => Ok(axum::response::Redirect::to(WORKSPACE).into_response()),
        Err(error) => {
            let status = error.0;
            let mut r = render(
                &app,
                &session,
                &Browse::default(),
                None,
                Feedback {
                    message: Some(error.1),
                    settings: Some(f),
                    ..Feedback::default()
                },
            )
            .await?;
            *r.status_mut() = status;
            Ok(r)
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportForm {
    csrf: String,
    version: i64,
    action: String,
    #[serde(default)]
    path: String,
}
async fn report_action(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<ReportForm>,
) -> Result<Response> {
    let session = admin_session(&app, &headers).await?;
    crate::auth::csrf(&session, &f.csrf)?;
    let result = match f.action.as_str() {
        "delete" => redirects::delete_missing(&app, &f.path, f.version).await,
        "purge" => redirects::purge_missing(&app, f.version).await,
        "alias" => redirects::remove_alias(&app, &f.path, f.version).await,
        _ => Err(Error::invalid("Choose an existing report action.")),
    };
    match result {
        Ok(()) => Ok(axum::response::Redirect::to(WORKSPACE).into_response()),
        Err(error) => {
            let status = error.0;
            let mut r = render(
                &app,
                &session,
                &Browse::default(),
                None,
                Feedback {
                    message: Some(error.1),
                    ..Feedback::default()
                },
            )
            .await?;
            *r.status_mut() = status;
            Ok(r)
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyInput {
    csrf: String,
    version: i64,
    settings: Settings,
}
async fn api_get(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    admin_session(&app, &headers).await?;
    let (policy, version) = redirects::load(&app).await?;
    Ok(Json(serde_json::json!({"version":version,"policy":policy})).into_response())
}
async fn api_save(
    State(app): State<App>,
    headers: HeaderMap,
    Json(f): Json<PolicyInput>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    crate::auth::csrf(&s, &f.csrf)?;
    let version = redirects::configure(&app, f.settings, f.version).await?;
    Ok(Json(serde_json::json!({"version":version})).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanForm {
    csrf: String,
    #[serde(default)]
    output: String,
}
fn edge_row(edge: &super::rendered_links::Edge) -> Markup {
    html! {div class="panel" {p {strong {(edge.state)}" · "(edge.element)" · "(edge.placement)}p {(edge.source)" → "(edge.target)}@if let Some(path)=&edge.destination {p class="muted" {"Resolved source: "(path)}}}}
}
fn scan_controls(session: &Session) -> Markup {
    html! {form method="post" action="/admin/discovery/rendered-links" {(view::csrf(session))div class="toolbar" {
        button name="output" value="html" {"Analyze rendered publications"}
        button class="secondary" name="output" value="json" {"Download full graph"}
    }}p class="muted" {"Canonical anonymous publications and language landing pages only. At most 1,000 publications, 20,000 edges, 2 MiB per page, 64 MiB aggregate HTML and a 20-second work budget. No external requests or client JavaScript. Unscanned targets remain explicit; a refusal produces no orphan conclusion."}}
}
async fn scan_page(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    Ok(axum::response::Html(view::layout("Rendered publication links",&app.db.settings().await?,Some(&s),html! {
        (view::heading("Discovery","Rendered publication links","Inspect the same anonymous HTML readers receive, including navigation and server-resolved theme bindings."))
        p {a href=(WORKSPACE) {"Redirect workspace"} " · " a href="/admin/discovery/links" {"Editorial link checks"}}
        (scan_controls(&s))
    })).into_response())
}
async fn scan_action(
    State(app): State<App>,
    headers: HeaderMap,
    Form(f): Form<ScanForm>,
) -> Result<Response> {
    let s = admin_session(&app, &headers).await?;
    crate::auth::csrf(&s, &f.csrf)?;
    if !["html", "json"].contains(&f.output.as_str()) {
        return Err(Error::invalid("Choose report or full graph download."));
    }
    let report = super::rendered_links::report(&app).await;
    if f.output == "json"
        && let Ok(report) = &report
    {
        return Ok((
            [
                (
                    "content-disposition",
                    "attachment; filename=wpalt-rendered-links.json",
                ),
                ("cache-control", "no-store"),
            ],
            Json(report),
        )
            .into_response());
    }
    let status = report
        .as_ref()
        .err()
        .map_or(axum::http::StatusCode::OK, |e| e.0);
    let body = html! {
        (view::heading("Discovery","Rendered publication links","Actual anonymous server output, with explicit target states and work limits."))
        p {a href=(WORKSPACE) {"Redirect workspace"}}
        (scan_controls(&s))
        @match report {
            Err(error)=>{div class="notice error" role="alert" {(error.1)}},
            Ok(report)=>{
                section class="panel" {h2 {"Completed declared graph"}
                    p {(report.sources.len())" sources · "(report.edges.len())" rendered edges · "(report.html_bytes)" HTML bytes · "(format!("{:.1}",report.elapsed_ms))" ms"}
                    p class="muted" {(report.scope)}
                    p {"No inbound anchors within this declared graph: "(report.without_inbound.len())". This is a bounded graph observation, not an indexing or whole-site orphan claim."}
                    details {summary {"Publications without inbound anchors"}@for path in &report.without_inbound {p {a href=(path) {(path)}}}}
                    details {summary {"Scanned sources"}@for source in &report.sources {p {(source.path)" · "(source.html_bytes)" bytes"}}}
                }
                section class="panel" {h2 {"Rendered edges"}
                    p class="muted" {"Review broken or refused targets first. Other targets include both resolved and explicitly unscanned states. Download the full graph for every edge, source digest and target state."}
                    @let problems: Vec<_> = report.edges.iter().filter(|e| e.state.starts_with("broken-") || e.state=="redirect-refused").collect();
                    p {(problems.len())" broken or refused edges · showing the first 100"}
                    @if problems.is_empty() {p {"No broken or refused targets in this declared graph."}}
                    @for edge in problems.iter().take(100) {(edge_row(edge))}
                    details {summary {"Other rendered targets · first 50"}
                        @for edge in report.edges.iter().filter(|e| !e.state.starts_with("broken-") && e.state!="redirect-refused").take(50) {(edge_row(edge))}
                    }
                }
            }
        }
    };
    Ok((
        status,
        axum::response::Html(view::layout(
            "Rendered publication links",
            &app.db.settings().await?,
            Some(&s),
            body,
        )),
    )
        .into_response())
}
pub fn routes() -> Router<App> {
    Router::new()
        .route(
            "/admin/discovery/rendered-links",
            get(scan_page).post(scan_action),
        )
        .route(WORKSPACE, get(page))
        .route(
            "/admin/discovery/routes/rule",
            axum::routing::post(rule_action),
        )
        .route(
            "/admin/discovery/routes/settings",
            axum::routing::post(settings_action),
        )
        .route(
            "/admin/discovery/routes/report",
            axum::routing::post(report_action),
        )
        .route(
            "/api/discovery/redirect-policy",
            get(api_get).post(api_save),
        )
}

/// Preserve the original version and native inputs after an exact-rule refusal.
pub(super) async fn exact_error(
    app: &App,
    session: &Session,
    form: super::RedirectForm,
    error: Error,
) -> Result<Response> {
    let status = error.0;
    let mut response = render(
        app,
        session,
        &Browse::default(),
        None,
        Feedback {
            message: Some(error.1),
            exact: Some(form),
            ..Feedback::default()
        },
    )
    .await?;
    *response.status_mut() = status;
    Ok(response)
}
