use crate::model::{NavItem, Post, Session, Settings};
use maud::{DOCTYPE, Markup, PreEscaped, html};

pub fn layout(title: &str, settings: &Settings, session: Option<&Session>, body: Markup) -> String {
    layout_inner(title, settings, session, body, false, None)
}
pub fn member_layout(title: &str, settings: &Settings, body: Markup) -> String {
    layout_inner(title, settings, None, body, true, None)
}
pub fn localized_layout(
    title: &str,
    settings: &Settings,
    session: &Session,
    body: Markup,
    locale: &str,
) -> String {
    layout_inner(title, settings, Some(session), body, false, Some(locale))
}
fn layout_inner(
    title: &str,
    settings: &Settings,
    session: Option<&Session>,
    body: Markup,
    member: bool,
    body_locale: Option<&str>,
) -> String {
    let admin = session.is_some();
    let catalog = session
        .and_then(|s| crate::platform::i18n::Catalog::select(&s.interface_locale).ok())
        .unwrap_or(crate::platform::i18n::Catalog::english());
    let content_locale = body_locale.unwrap_or("en");
    let content_direction = if body_locale.is_some() {
        catalog.direction()
    } else {
        "ltr"
    };
    let nav: Vec<NavItem> = serde_json::from_str(&settings.navigation).unwrap_or_default();
    html! { (DOCTYPE) html lang=(catalog.locale()) dir=(catalog.direction()) { head {
        meta charset="utf-8"; meta name="viewport" content="width=device-width, initial-scale=1";
        title lang=(content_locale) {(title) " · " (settings.title)}
        meta name="description" content=(settings.description);
        link rel="stylesheet" href="/assets/app.css";
        @if admin || member || title == "Sign in" {link rel="stylesheet" href="/assets/admin-ui.css";}
        @if admin {script defer src="/assets/admin.js" {}}
        link rel="alternate" type="application/rss+xml" title=(settings.title) href="/feed.xml";
    } body class=(if admin{"admin"}else if member{"member"}else if title == "Sign in" {"auth"}else{settings.theme.as_str()}) {
        a class="skip" href="#main" {(catalog.text("nav.skip"))}
        @if let Some(s)=session {
            aside class="sidebar" {
                a class="brand" href="/admin" {span class="brand-mark" {"w"} "wpalt"}
                p class="sidebar-note" {(catalog.text("nav.hint"))}
                nav aria-label=(catalog.text("nav.admin")) {
                    a href="/admin" {(catalog.text("nav.overview"))}
                    @if s.can_edit() {a href="/admin/posts" {(catalog.text("nav.content"))} a href="/admin/editorial" {(catalog.text("nav.editorial"))} a href="/admin/languages" {(catalog.text("nav.languages"))} a href="/admin/media" {(catalog.text("nav.media"))} @if settings.business_enabled {a href="/admin/forms" {(catalog.text("nav.forms"))} a href="/admin/audience" {(catalog.text("nav.audience"))} a href="/admin/mail" {(catalog.text("nav.mail"))} @if settings.engagement_available && s.is_admin() {a href="/admin/engagement" {(catalog.text("nav.engagement"))}}}}
                    @if s.is_admin() && settings.membership_enabled {a href="/admin/members" {(catalog.text("nav.members"))}a href="/admin/courses" {(catalog.text("nav.courses"))}}
                    @if s.is_admin() && settings.commerce_enabled {a href="/admin/shop" {(catalog.text("nav.commerce"))}}
                    @if s.can_moderate() {a href="/admin/comments" {(catalog.text("nav.comments"))}}
                    @if s.is_admin() {a href="/admin/builder" {(catalog.text("nav.studio"))} a href="/admin/discovery" {(catalog.text("nav.discovery"))} a href="/admin/settings" {(catalog.text("nav.settings"))} a href="/admin/operations" {(catalog.text("nav.operations"))}}
                    a href="/account/security" {(catalog.text("nav.account"))} a href="/account/interface" {(catalog.text("nav.interface"))}a href="/" {(catalog.text("nav.site"))}
                }
                div class="account" {strong {bdi {(s.user.name)}} span lang="en" dir="ltr" {(s.user.role)}
                    form method="post" action="/logout" {input type="hidden" name="csrf" value=(s.csrf);button class="quiet" {(catalog.text("account.sign_out"))}}
                }
            }
        } @else {
            header class="site-header" {a class="site-brand" href="/" {(settings.title)} nav aria-label="Website" {
                @for item in nav {a href=(item.url) {(item.label)}}
                a href="/search" {"Search"}
            }}
        }
        main id="main" lang=(content_locale) dir=(content_direction) class=(if admin || member{"workspace"}else{"site-main"}) {(body)}
        @if !admin {footer class="site-footer" {span {(settings.description)} a href=(if member {"/account"}else{"/login"}) {(if member {"Account"}else{"Manage site"})}}}
    }} }.into_string()
}
pub fn heading(kicker: &str, title: &str, description: &str) -> Markup {
    html! {header class="page-heading" {p class="eyebrow" {(kicker)} h1 {(title)} p {(description)}}}
}
pub fn csrf(s: &Session) -> Markup {
    html! {input type="hidden" name="csrf" value=(s.csrf);}
}
pub fn public_body(post: &Post, preview: bool) -> Markup {
    public_body_with_dimensions(post, preview, &std::collections::BTreeMap::new())
}
pub fn public_body_with_dimensions(
    post: &Post,
    preview: bool,
    dimensions: &std::collections::BTreeMap<String, (u32, u32)>,
) -> Markup {
    let title = if preview {
        &post.title
    } else {
        &post.published_title
    };
    let fields = if preview {
        &post.fields
    } else {
        &post.published_fields
    };
    let document = if preview {
        &post.document
    } else {
        &post.published_document
    };
    let fields: serde_json::Value = serde_json::from_str(fields).unwrap_or_default();
    let rendered = crate::document::Document::parse(document)
        .map(|mut d| {
            d.apply_dimensions(dimensions);
            if preview { d.preview_html() } else { d.html() }
        })
        .unwrap_or_default();
    html! { article class="article" {
        @if preview {p class="notice" {"Private preview — unpublished changes. " a href=(format!("/admin/posts/{}",post.id)) {"Back to editor"}}}
        p class="eyebrow" {(post.kind)} h1 {(title)}
        @if let Some(subtitle)=fields.get("subtitle").and_then(|v|v.as_str()) {p class="lead" {(subtitle)}}
        div class="prose" {(PreEscaped(rendered))}
    }}
}
pub fn login(settings: &Settings, identity: bool) -> String {
    layout(
        "Sign in",
        settings,
        None,
        html! {section class="login-card panel" {p class="eyebrow" {"Owner-controlled publishing"}h1 {"Welcome back."}p {"Sign in to your account."}
            form method="post" action="/login" {label {"Email" input type="email" name="email" autocomplete="username" required;}
                label {"Password" input type="password" name="password" autocomplete="current-password" required maxlength="256";}
                label {"Authenticator or recovery code · if enabled" input name="code" autocomplete="one-time-code" maxlength="24";}
                button {"Sign in"}}
            form data-passkey="login" {label {"Passkey account" input type="email" name="email" required autocomplete="username" maxlength="254";}button class="secondary" {"Use a passkey"}p role="status" aria-live="polite" {}}
                script defer src="/assets/auth.js" {}
                @if identity {p {a href="/members/identity/start" {"Sign in with your identity provider"}}}
            p class="muted" {"No external account required."}
        }},
    )
}

/// Plain-text teasers preserve words without exposing Markdown/HTML syntax.
pub fn excerpt(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser};
    let mut text = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Text(value) | Event::Code(value) => text.push_str(&value),
            Event::SoftBreak | Event::HardBreak | Event::End(_) => text.push(' '),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Bound work before Markdown parsing, preserve complete final words, and make truncation explicit.
pub fn bounded_excerpt(markdown: &str, source_truncated: bool) -> String {
    let mut chars = markdown.chars();
    let raw: String = chars.by_ref().take(220).collect();
    let truncated = source_truncated || chars.next().is_some();
    let mut plain = excerpt(&raw);
    if truncated {
        if let Some((whole, _)) = plain.rsplit_once(char::is_whitespace) {
            plain = whole.trim_end().to_owned();
        }
        plain.push('…');
    }
    plain
}

/// Display server timestamps with an explicit zone and a useful empty state.
pub fn timestamp(seconds: i64) -> String {
    if seconds == 0 {
        return "Not recorded".into();
    }
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|date| date.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Invalid timestamp".into())
}
