# ADR 0018 — Theme-owned navigation and bounded local assets

Date: 2026-10-07. Status: implemented and locally verified; final delivery gates pending.

## Decision

Named navigation families live with the theme draft/live package and share its optimistic version, preview, deliberate publication, revision history and recovery. Navigation nodes explicitly select a family or the existing site-links feature. Site links remain immediate settings; the studio explains that distinction. Families admit labelled links/groups, descriptions, up to four levels, 32 siblings, 128 items per language tree, 512 across variants, and eight families per theme. Names use native identifiers. Safe URL checks and Maud escaping remain authoritative; rendered items count against the shared 5,000-node budget.

Families explicitly declare actual default language/direction. Configured variants do not silently replace public language settings. Saving checks configured language/direction; removing or changing a language used by current draft/live navigation is refused. Historical revisions remain recoverable drafts subject to current validation. Native lists/disclosures provide progressive navigation without requiring scripts; any Escape/focus enhancement requires real keyboard/no-JavaScript/preview testing. Ordinary links do not receive application-menu roles.

HTTP theme save/publication, revision restoration and activation retain the admitted administrator and recheck current role/session after acquiring the shared mutation guard. A stale or revoked session cannot commit simply because it passed an earlier request check. Stopped owner CLI operations keep their separate trusted local authority.

Typography and asset implementation retains immutable, deduplicated local asset bytes separate from repeatedly stored theme revisions. Versioned packages refer to assets rather than embedding repeated font payloads in every draft/history/cache entry. Private preview and admitted published references determine visibility. The parser, font/container limits, body/read budgets, license/provenance metadata and recovery/import/export representation must be concrete before delivery. Do not treat a filename or header signature as complete font validation; declare what table/container validation actually covers and test browser font readiness independently.

Expanded owner styles use bounded declarative properties compiled into scoped local stylesheets. Native CSP remains local-only and does not grant remote imports, event/script execution or unrelated file access. Admitting controlled styles must not be advertised as unrestricted raw CSS compatibility. A package/native/portable grammar transition requires an explicit stopped migration and the preserved actual schema-18 executable for fresh old-runtime rollback, not runtime legacy fallback.

## Alternatives and consequences

Saving hierarchical menus into immediate site settings would fragment the preview/publication experience and expose drafts; rejected. Repeating embedded font data in every revision would increase database, cache and export work; prefer a shared asset identity. Executable HTML/menu widgets and arbitrary network loaders would exceed this package's authority. Initial container/style breadth remains explicit and is recorded in the backlog. Real-time collaboration, full Elementor widget/style adaptation and arbitrary isolated extension code remain their own packages.

## Verification

One connected SQLite/PostgreSQL journey initially passes preview/live separation, exact stale-publication refusal, escaped labels, native disclosure markup, language variants and fresh recovery. A refinement adds current-language dependency refusal and deterministic commit-time administrator revocation. Actual browser geometry, keyboard, fonts, asset security, resource observations, cumulative recovery/migration and final delivery gates remain pending; synthetic HTML assertions do not complete the milestone.

Primary references are listed in the D04 contract and refreshed through the feature-guidance protocol. App licensing and owner private font rights are separate from the accompanying test fixture's unmodified SIL OFL 1.1 license.

## Delivered grammar and transition

Package 2 provides at most 32 named reusable styles, with explicit attach/detach behavior and bounded typography, spacing, layout, colors, alignment, border and radius. A node chooses a named style or independent declarations, never both. Controlled CSS is compiled; arbitrary raw CSS is refused. Named navigation also supports grouped columns and per-language direction.

Static TrueType faces are inspected off the asynchronous request executor with container/directory/checksum/table/glyph/component bounds and cycle/depth checks. WOFF/WOFF2, CFF, collections, variable/color/SVG and embedded bitmap containers are refused. The parser does not execute hinting, shaping or rasterization and is not advertised as a complete font sanitizer. Admission requires explicit owner distribution rights/provenance. Immutable SHA-256 blobs are capped at 2 MiB each, 128 assets/16 MiB total, with at most eight declared faces per theme. A separate reference index covers retained revision usage. Normal bounded history retention removes obsolete references; manual history pruning and configurable storage ceilings remain future breadth.

Saved theme bundles contain exactly referenced fonts, source/license metadata and the native package; content/media/form dependencies must already exist. Inspection and current-owner rechecking precede one transaction for asset admission, optimistic theme save and reference updates. Failed/stale imports leave no partial font/theme graph. HTTP import defaults to a draft; stopped CLI authority remains independent. Fresh recovery v15 preserves the full graph. Native schema 19 has explicit stopped migration from 14–18 and package-1 conversion; no old runtime grammar branch is added.

Public font delivery requires a current published reference; private draft fonts require a current administrator. Conditional responses recheck visibility and read bounded metadata without loading bytes. Previously published bytes can remain in a visitor cache after withdrawal: server visibility cannot revoke copies already delivered. Theme-only export stays private. The detailed [verification ledger](../evidence/d04-verification.md) separates connected native/browser checks, resource observations, actual schema-18 rollback, and pending CI/release gates.

Large bundle requests authenticate before body reads; bounded work admission precedes buffering. JSON parsing, canonical byte-budget counting, font inspection and export serialization run off asynchronous request executors. Blocking tasks retain owned permits through actual completion, including after caller cancellation; request timeout cannot replenish their inspection capacity early.
