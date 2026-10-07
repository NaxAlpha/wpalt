//! Durable own-account interface preferences; never content/publication authority.
use super::i18n::Catalog;
use crate::{
    App, auth,
    error::{Error, Result},
    view,
};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Form, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use maud::html;
use serde::Deserialize;

pub const MAX_VERSION: i64 = 1_000_000_000;
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS user_preferences(user_id TEXT PRIMARY KEY REFERENCES users(id),locale TEXT NOT NULL CHECK(locale IN ('en','fr','ja','ar')),version BIGINT NOT NULL CHECK(version>0 AND version<=1000000000)); CREATE INDEX IF NOT EXISTS language_workspace_all ON posts(updated_at DESC,id DESC); CREATE INDEX IF NOT EXISTS language_workspace_cursor ON posts(locale,updated_at DESC,id DESC);";
pub fn routes() -> Router<App> {
    Router::new()
        .route("/account/interface", get(page).post(save))
        .layer(DefaultBodyLimit::max(4096))
}
async fn page(State(app): State<App>, headers: HeaderMap) -> Result<Html<String>> {
    let s = auth::session(&app, &headers).await?;
    render(&app, &s, false).await
}
async fn render(app: &App, s: &crate::model::Session, conflict: bool) -> Result<Html<String>> {
    let catalog = Catalog::select(&s.interface_locale).unwrap_or(Catalog::english());
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM user_preferences WHERE user_id=$1")
            .bind(&s.user.id)
            .fetch_optional(&app.db.pool)
            .await?;
    let body = html! {
        (view::heading(catalog.text("interface.kicker"),catalog.text("interface.title"),catalog.text("interface.help")))
        @if conflict {p class="notice error" role="alert" {(catalog.text("interface.conflict"))}}
        form method="post" class="panel" {(view::csrf(s)) input type="hidden" name="version" value=(version.unwrap_or(0));
            label {(catalog.text("interface.title")) select name="locale" aria-label=(catalog.text("interface.title")) {
                @for (code,label) in [("en","English"),("fr","Français"),("ja","日本語"),("ar","العربية")]{option value=(code) lang=(code) selected[s.interface_locale==code] {(label)}}
            }}
            button {(catalog.text("interface.save"))}
        }
    };
    Ok(Html(view::localized_layout(
        catalog.text("interface.title"),
        &app.db.settings().await?,
        s,
        body,
        catalog.locale(),
    )))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    csrf: String,
    locale: String,
    version: i64,
}
async fn save(
    State(app): State<App>,
    headers: HeaderMap,
    Form(input): Form<Preference>,
) -> Result<Response> {
    let _guard = app.mutation().await?;
    // Reload after acquiring authority: revoked/disabled sessions cannot change retained state.
    let s = auth::session(&app, &headers).await?;
    auth::csrf(&s, &input.csrf)?;
    Catalog::select(&input.locale)?;
    if input.version < 0 || input.version >= MAX_VERSION {
        return Err(Error::invalid("Review the current interface preference."));
    }
    let mut tx = app.db.pool.begin().await?;
    let affected = if input.version == 0 {
        sqlx::query("INSERT INTO user_preferences(user_id,locale,version) VALUES($1,$2,1) ON CONFLICT(user_id) DO NOTHING").bind(&s.user.id).bind(&input.locale).execute(&mut *tx).await?.rows_affected()
    } else {
        sqlx::query("UPDATE user_preferences SET locale=$2,version=version+1 WHERE user_id=$1 AND version=$3").bind(&s.user.id).bind(&input.locale).bind(input.version).execute(&mut *tx).await?.rows_affected()
    };
    if affected != 1 {
        drop(tx);
        let fresh = auth::session(&app, &headers).await?;
        return Ok((StatusCode::CONFLICT, render(&app, &fresh, true).await?).into_response());
    }
    tx.commit().await?;
    tracing::debug!(
        event = "interface_preference_saved",
        locale = input.locale,
        version = input.version + 1
    );
    Ok(Redirect::to("/account/interface").into_response())
}
