//! Anonymous, bounded projections for explicit private-device saving.
use crate::{
    App,
    document::{Document, Node},
    error::{Error, Result},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderValue, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

pub const MAX_SNAPSHOT: usize = 512 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub format: String,
    pub kind: String,
    pub id: String,
    pub title: String,
    pub source: String,
    pub version: String,
    pub locale: String,
    pub text: String,
    pub definition: Option<Value>,
    pub form_version: i64,
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/offline/", get(shell))
        .route("/offline/worker.js", get(worker))
        .route("/offline/app.js", get(script))
        .route("/offline/style.css", get(style))
        .route("/offline/forms.js", get(forms))
        .route("/offline/snapshots/{kind}/{id}", get(snapshot))
}
fn private(mut response: Response) -> Response {
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    response
}
async fn shell() -> Response {
    private(Html(include_str!("../../assets/offline.html")).into_response())
}
async fn worker() -> Response {
    private(
        (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            include_str!("../../assets/generated/offline-worker.js"),
        )
            .into_response(),
    )
}
async fn script() -> Response {
    private(
        (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            include_str!("../../assets/generated/offline.js"),
        )
            .into_response(),
    )
}
async fn style() -> Response {
    private(
        (
            [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
            include_str!("../../assets/generated/offline.css"),
        )
            .into_response(),
    )
}
async fn forms() -> Response {
    private(
        (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            include_str!("../../assets/generated/forms.js"),
        )
            .into_response(),
    )
}
async fn snapshot(
    State(app): State<App>,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Response> {
    let result = project(&app, &kind, &id).await?;
    let mut response = private(Json(result).into_response());
    response.headers_mut().insert(
        "x-wpalt-offline",
        HeaderValue::from_static("public-snapshot-v1"),
    );
    Ok(response)
}
/// One consistent read snapshot includes anonymous access policy. Never render with cookies.
pub async fn project(app: &App, kind: &str, id: &str) -> Result<Snapshot> {
    crate::membership::uuid(id)?;
    let mut tx = app.db.pool.begin().await?;
    if app.db.postgres {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let result = match kind {
        "article" => {
            let sql = format!(
                "SELECT published_title,published_slug,published_locale,published_document FROM posts WHERE id=$1 AND published_slug<>'' AND status='published'{}",
                crate::membership::PUBLIC_POST
            );
            let row = sqlx::query(&sql)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(Error::not_found)?;
            let document = Document::parse(&row.get::<String, _>("published_document"))?;
            let mut text = String::new();
            fn visit(n: &Node, out: &mut String) {
                if let Some(t) = &n.text {
                    out.push_str(t);
                }
                match n.kind.as_str() {
                    "image" => {
                        out.push_str("\n[Image requires an online visit]\n");
                        return;
                    }
                    "form" | "video" | "embed" => {
                        out.push_str("\n[Interactive content requires an online visit]\n");
                        return;
                    }
                    "hard_break" => out.push('\n'),
                    _ => {}
                }
                for child in &n.content {
                    visit(child, out);
                }
                if ["paragraph", "heading", "list_item", "table_row"].contains(&n.kind.as_str()) {
                    out.push('\n');
                }
            }
            visit(&document.root, &mut text);
            let locale: String = row.get("published_locale");
            let slug: String = row.get("published_slug");
            Snapshot {
                format: "wpalt-public-snapshot-v1".into(),
                kind: kind.into(),
                id: id.into(),
                title: row.get("published_title"),
                source: if locale == "en" {
                    format!("/{slug}")
                } else {
                    format!("/{locale}/{slug}")
                },
                version: String::new(),
                locale,
                text,
                definition: None,
                form_version: 0,
            }
        }
        "form" if app.config.business_enabled => {
            let row = sqlx::query("SELECT live,published_version FROM business_forms WHERE id=$1 AND published_version>0").bind(id).fetch_optional(&mut *tx).await?.ok_or_else(Error::not_found)?;
            let mut publication: crate::business::store::PublishedForm =
                serde_json::from_str(&row.get::<String, _>("live"))
                    .map_err(|_| Error::invalid("Stored form needs repair."))?;
            use crate::business::forms::Widget;
            if publication.form.registration.is_some()
                || publication.form.subscription.is_some()
                || publication.form.fields.iter().any(|f| {
                    !["string", "number", "boolean"].contains(&f.schema.kind.as_str())
                        || matches!(
                            f.widget,
                            Some(
                                Widget::Upload
                                    | Widget::Signature { .. }
                                    | Widget::Acknowledgment { .. }
                            )
                        )
                })
            {
                return Err(Error::invalid(
                    "This form has account, consent or private attachment inputs. Visit the live form online.",
                ));
            }
            publication.form.notifications.clear();
            publication.form.draft_post = None;
            let title = publication.form.title.clone();
            Snapshot {format:"wpalt-public-snapshot-v1".into(),kind:kind.into(),id:id.into(),title,source:format!("/forms/{id}"),version:String::new(),locale:"en".into(),text:"Edit a device draft here. Reconnect and review the current publication before sending on the live form.".into(),definition:Some(serde_json::to_value(publication).map_err(|_|Error::invalid("Invalid public form."))?),form_version:row.get("published_version")}
        }
        _ => return Err(Error::not_found()),
    };
    tx.commit().await?;
    let mut result = result;
    let bytes =
        serde_json::to_vec(&result).map_err(|_| Error::invalid("Invalid public snapshot."))?;
    if bytes.len() > MAX_SNAPSHOT - 128 {
        return Err(Error::invalid(
            "This publication exceeds the offline snapshot budget.",
        ));
    }
    result.version = crate::auth::digest(&bytes);
    Ok(result)
}
