//! Private editorial queue and native authoring controls share server primitives.
use crate::{
    App, auth, editorial,
    error::{Error, Result},
    model::Session,
    schema::Registry,
    view,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Form, Path, Query, State},
    http::HeaderMap,
    response::{Html, Redirect},
    routing::{get, post},
};
use maud::{Markup, html};
use serde::Deserialize;
use sqlx::{Any, QueryBuilder, Row, any::AnyRow};

pub fn routes() -> Router<App> {
    Router::new()
        .route("/admin/editorial", get(queue))
        .route("/admin/editorial/{id}", post(decision_form))
        .route("/api/admin/editorial/{id}", get(read).post(transition))
        .layer(DefaultBodyLimit::max(8192))
}
async fn session(app: &App, headers: &HeaderMap) -> Result<Session> {
    let s = auth::session(app, headers).await?;
    if !s.can_edit() || s.hash.starts_with("integration:") {
        return Err(Error::forbidden());
    }
    Ok(s)
}
#[derive(Default)]
pub(crate) struct Panel {
    pub work: Option<editorial::Work>,
    reviewers: Vec<AnyRow>,
    history: Vec<AnyRow>,
    required: bool,
}
impl Panel {
    pub async fn load(app: &App, s: &Session, id: Option<&str>, kind: &str) -> Result<Self> {
        let registry = Registry::load(app).await?;
        let required = registry.models.get(kind).is_some_and(|m| m.review_required);
        // Reviewer choices are paginated on the queue; a bounded authoring list avoids a full user scan.
        let reviewers=sqlx::query("SELECT id,name,email FROM users WHERE role IN ('admin','editor') AND id<>$1 ORDER BY name,id LIMIT 128").bind(&s.user.id).fetch_all(&app.db.pool).await?;
        let (work, history) = if let Some(id) = id {
            (editorial::get(app,id).await?,sqlx::query("SELECT action,notes,created_at FROM editorial_decisions WHERE post_id=$1 ORDER BY workflow_version DESC,id DESC LIMIT 20").bind(id).fetch_all(&app.db.pool).await?)
        } else {
            (None, vec![])
        };
        Ok(Self {
            work,
            reviewers,
            history,
            required,
        })
    }
    pub fn request_controls(&self) -> Markup {
        html! {
         details data-editorial-pane hidden[!self.required] {summary {"Editorial review"}
          p {"Save your current work and send it to another editor. Publication requires approval of this exact working copy."}
          p role="status" data-editorial-state {(self.work.as_ref().map(|w|w.state.as_str()).unwrap_or("Save a draft to begin"))}
          label {"Reviewer" select name="reviewer_id" aria-label="Reviewer" {option value="" {"Choose an editor"} @for r in &self.reviewers {option value=(r.get::<String,_>("id")) selected[self.work.as_ref().is_some_and(|w|w.assigned_to==r.get::<String,_>("id"))] disabled[self.work.as_ref().is_some_and(|w|w.edited_by==r.get::<String,_>("id"))] {(r.get::<String,_>("name")) " · " (r.get::<String,_>("email"))}}}}
          small {"Up to 128 active accounts are shown. The last editor cannot approve their own material."}
          label {"Private review note" textarea name="review_note" aria-label="Private review note" maxlength="2000" {} small {"Only editors and administrators can read this note."}}
          button class="secondary" name="action" value="request_review" {"Save & request review"}
          a href="/admin/editorial" {"Open editorial queue"}
         }
        }
    }
    pub fn decisions(&self, s: &Session, id: Option<&str>) -> Markup {
        html! {
         @if let (Some(id),Some(w))=(id,&self.work) {
          section class="panel" data-editorial-decisions {h2 {"Review feedback"}p class="muted" {"Decisions apply to the saved working copy. Editing requires a new review."}
           p data-review-note {(w.notes)}
           @if self.required && ((w.state=="pending"&&w.assigned_to==s.user.id) || (["pending","approved"].contains(&w.state.as_str())&&w.requested_by==s.user.id)) {
            form method="post" action=(format!("/admin/editorial/{id}")) data-review-decision {
             (view::csrf(s)) input type="hidden" name="content_version" value=(w.content_version); input type="hidden" name="workflow_version" value=(w.version);
             label {"Decision note" textarea name="notes" maxlength="2000" {}}
             div class="toolbar" {
              @if w.state=="pending"&&w.assigned_to==s.user.id {button name="action" value="approve" {"Approve saved content"}button class="secondary" name="action" value="changes" {"Request changes"}}
              @if w.requested_by==s.user.id {button class="secondary" name="action" value="withdraw" {"Withdraw review"}}
             }
            }
           }
           details {summary {"Recent review decisions"} @for r in &self.history {p {strong {(r.get::<String,_>("action"))} " · " (r.get::<String,_>("notes"))}}}
          }
         }
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    csrf: String,
    content_version: i64,
    workflow_version: i64,
    action: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    reviewer_id: String,
}
async fn apply(app: &App, s: &Session, id: &str, d: &Decision) -> Result<editorial::Work> {
    auth::csrf(s, &d.csrf)?;
    if d.action == "request" {
        editorial::request(
            app,
            s,
            id,
            d.content_version,
            d.workflow_version,
            &editorial::Request {
                reviewer_id: d.reviewer_id.clone(),
                notes: d.notes.clone(),
            },
        )
        .await
    } else {
        editorial::decide(
            app,
            s,
            id,
            d.content_version,
            d.workflow_version,
            &d.action,
            &d.notes,
        )
        .await
    }
}
async fn decision_form(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(d): Form<Decision>,
) -> Result<Redirect> {
    let s = session(&app, &headers).await?;
    apply(&app, &s, &id, &d).await?;
    Ok(Redirect::to(&format!("/admin/posts/{id}")))
}
async fn transition(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(d): Json<Decision>,
) -> Result<Json<editorial::Work>> {
    let s = session(&app, &headers).await?;
    Ok(Json(apply(&app, &s, &id, &d).await?))
}
async fn read(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Option<editorial::Work>>> {
    session(&app, &headers).await?;
    crate::content::get(&app, &id).await?;
    Ok(Json(editorial::get(&app, &id).await?))
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Filter {
    #[serde(default)]
    state: String,
    #[serde(default)]
    scope: String,
    after: Option<String>,
}
async fn queue(
    State(app): State<App>,
    headers: HeaderMap,
    Query(f): Query<Filter>,
) -> Result<Html<String>> {
    let s = session(&app, &headers).await?;
    let state = if f.state.is_empty() {
        "pending"
    } else {
        &f.state
    };
    let scope = if f.scope.is_empty() {
        "assigned"
    } else {
        &f.scope
    };
    if ![
        "draft",
        "pending",
        "approved",
        "changes",
        "scheduled",
        "published",
    ]
    .contains(&state)
        || !["assigned", "requested", "all"].contains(&scope)
    {
        return Err(Error::invalid("Choose a valid review state and queue."));
    }
    let mut q = QueryBuilder::<Any>::new(
        "SELECT w.post_id,w.state,w.notes,w.requested_at,w.version,p.title,p.kind FROM editorial_work w JOIN posts p ON p.id=w.post_id WHERE w.state=",
    );
    q.push_bind(state);
    match scope {
        "assigned" => {
            q.push(" AND w.assigned_to=").push_bind(&s.user.id);
        }
        "requested" => {
            q.push(" AND w.requested_by=").push_bind(&s.user.id);
        }
        _ => {}
    }
    if let Some(after) = &f.after {
        let (time, id) = after
            .split_once(':')
            .ok_or_else(|| Error::invalid("Invalid queue cursor."))?;
        let time: i64 = time
            .parse()
            .map_err(|_| Error::invalid("Invalid queue cursor."))?;
        if time < 0 || uuid::Uuid::parse_str(id).is_err() {
            return Err(Error::invalid("Invalid queue cursor."));
        }
        q.push(" AND (w.requested_at,w.post_id)<(")
            .push_bind(time)
            .push(",")
            .push_bind(id)
            .push(")");
    }
    q.push(" ORDER BY w.requested_at DESC,w.post_id DESC LIMIT 41");
    let rows = app.db.fetch_builder(&mut q).await?;
    let body = html! {
     (view::heading("Publishing","Editorial queue","Review saved content, respond privately and return to the shared editor."))
     form method="get" class="toolbar" {label {"Queue" select name="scope" {option value="assigned" selected[scope=="assigned"] {"Assigned to me"}option value="requested" selected[scope=="requested"] {"Requested by me"}option value="all" selected[scope=="all"] {"All editorial work"}}}label {"Review state" select name="state" {@for value in ["pending","approved","changes","draft","scheduled","published"] {option value=(value) selected[state==value] {(value)}}}}button class="secondary" {"Show work"}}
     section class="panel" {@if rows.is_empty(){p {"No saved work in this queue."}} @for r in rows.iter().take(40){article {h2 {a href=(format!("/admin/posts/{}",r.get::<String,_>("post_id"))) {(r.get::<String,_>("title"))}}p {span class="status" {(r.get::<String,_>("state"))} " · " (r.get::<String,_>("kind"))}p {(r.get::<String,_>("notes"))}}}}
     @if rows.len()>40 {@let r=&rows[39];a class="button secondary" href=(format!("/admin/editorial?scope={scope}&state={state}&after={}:{}",r.get::<i64,_>("requested_at"),r.get::<String,_>("post_id"))) {"Older review requests →"}}
    };
    Ok(Html(view::layout(
        "Editorial queue",
        &app.db.settings().await?,
        Some(&s),
        body,
    )))
}
