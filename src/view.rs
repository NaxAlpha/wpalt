use crate::{
    content,
    model::{Block, NavItem, Post, Session, Settings},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};

pub fn layout(title: &str, settings: &Settings, session: Option<&Session>, body: Markup) -> String {
    let admin = session.is_some();
    let nav: Vec<NavItem> = serde_json::from_str(&settings.navigation).unwrap_or_default();
    html! { (DOCTYPE) html lang="en" { head {
        meta charset="utf-8"; meta name="viewport" content="width=device-width, initial-scale=1";
        title {(title) " · " (settings.title)}
        meta name="description" content=(settings.description);
        link rel="stylesheet" href="/assets/app.css";
        @if admin || title == "Sign in" {link rel="stylesheet" href="/assets/admin-ui.css";}
        @if admin {script defer src="/assets/admin.js" {}}
        link rel="alternate" type="application/rss+xml" title=(settings.title) href="/feed.xml";
    } body class=(if admin{"admin"}else if title == "Sign in" {"auth"}else{settings.theme.as_str()}) {
        a class="skip" href="#main" {"Skip to content"}
        @if let Some(s)=session {
            aside class="sidebar" {
                a class="brand" href="/admin" {span class="brand-mark" {"w"} "wpalt"}
                p class="sidebar-note" {"Your site. Your server."}
                nav aria-label="Administration" {
                    a href="/admin" {"Overview"}
                    @if s.can_edit() {a href="/admin/posts" {"Content"} a href="/admin/media" {"Media library"}}
                    @if s.can_moderate() {a href="/admin/comments" {"Comments"}}
                    @if s.is_admin() {a href="/admin/builder" {"Design studio"} a href="/admin/discovery" {"Discovery"} a href="/admin/settings" {"Site settings"} a href="/admin/operations" {"Operations"}}
                    a href="/" {"View website ↗"}
                }
                div class="account" {strong {(s.user.name)} span {(s.user.role)}
                    form method="post" action="/logout" {input type="hidden" name="csrf" value=(s.csrf);button class="quiet" {"Sign out"}}
                }
            }
        } @else {
            header class="site-header" {a class="site-brand" href="/" {(settings.title)} nav aria-label="Website" {
                @for item in nav {a href=(item.url) {(item.label)}}
                a href="/search" {"Search"}
            }}
        }
        main id="main" class=(if admin{"workspace"}else{"site-main"}) {(body)}
        @if !admin {footer class="site-footer" {span {(settings.description)} a href="/login" {"Manage site"}}}
    }} }.into_string()
}
pub fn heading(kicker: &str, title: &str, description: &str) -> Markup {
    html! {header class="page-heading" {p class="eyebrow" {(kicker)} h1 {(title)} p {(description)}}}
}
pub fn csrf(s: &Session) -> Markup {
    html! {input type="hidden" name="csrf" value=(s.csrf);}
}
pub fn public_body(post: &Post, preview: bool) -> Markup {
    let title = if preview {
        &post.title
    } else {
        &post.published_title
    };
    let body = if preview {
        &post.body
    } else {
        &post.published_body
    };
    let fields = if preview {
        &post.fields
    } else {
        &post.published_fields
    };
    let blocks = if preview {
        &post.blocks
    } else {
        &post.published_blocks
    };
    let fields: serde_json::Value = serde_json::from_str(fields).unwrap_or_default();
    let blocks: Vec<Block> = serde_json::from_str(blocks).unwrap_or_default();
    html! { article class="article" {
        @if preview {p class="notice" {"Private preview — unpublished changes. " a href=(format!("/admin/posts/{}",post.id)) {"Back to editor"}}}
        p class="eyebrow" {(post.kind)} h1 {(title)}
        @if let Some(subtitle)=fields.get("subtitle").and_then(|v|v.as_str()) {p class="lead" {(subtitle)}}
        div class="prose" {(PreEscaped(content::markdown(body)))}
        @for block in blocks { @match block.kind.as_str() {
            "heading"=>{h2 {(block.text)}}
            "callout"=>{aside class="callout" {(block.text)}}
            _=>{div class="prose" {(PreEscaped(content::markdown(&block.text)))}}
        }}
    }}
}
pub fn login(settings: &Settings) -> String {
    layout(
        "Sign in",
        settings,
        None,
        html! {section class="login-card panel" {p class="eyebrow" {"Owner-controlled publishing"}h1 {"Welcome back."}p {"Sign in to manage your website."}
            form method="post" action="/login" {label {"Email" input type="email" name="email" autocomplete="username" required;}
                label {"Password" input type="password" name="password" autocomplete="current-password" required maxlength="256";}
                button {"Sign in"}}
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
