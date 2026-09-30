# ADR 0002 — typed composition and a bundled admin island

Date: 2026-10-01. Status: accepted for M2, subject to behavioral/resource verification.

Use a restricted versioned declarative theme graph with reusable parameterized components, responsive layouts/tokens, typed bindings, dynamic collections/repeaters, conditions and accessible widgets. Rust validates/resolves/renders the graph identically for authenticated preview and live output. It grants no arbitrary scripting/filesystem/network capability. Shared typed definitions power structured editing, relationships, options and bindings.

Use Preact 11.0.0 and esbuild 0.28.2 for a small, bundled admin island rather than replacing public pages with a frontend application. npm is a locked development toolchain; generated assets are committed, verified/rebuilt in CI and embedded in Rust. Runtime installation remains one application with bundled assets and no Node/provider account. No React compatibility layer is required. Primary current [Preact v11 guide](https://preactjs.com/guide/v11/getting-started/) and [hooks](https://preactjs.com/guide/v11/hooks/) were read; package versions came from their npm publisher metadata on this date.

Theme changes use draft/version/live snapshots, explicit publication/activation and bounded revisions. Public styles refer to published versions so in-flight documents are not broken by a theme switch; retain a bounded published history, not old-format runtime interpretation. Rendering/query work is bounded, dependency resolution is bulk/request-local, and compiled caches never contain private rendered content.

Implement schema-1 data preservation as a one-off schema-2 migration, then use only the current runtime model. SQLite constraint changes follow its [recreate/copy/check procedure](https://www.sqlite.org/lang_altertable.html#otheralter); PostgreSQL uses transactional DDL. Current-format recovery covers models/options/themes; old backups must be restored with M1 then upgraded, not silently parsed as the new format.

Current WordPress already has [Block Bindings](https://developer.wordpress.org/block-editor/reference-guides/block-api/block-bindings/) and [global theme settings/styles](https://developer.wordpress.org/themes/global-settings-and-styles/). The intended improvement is tighter integration, typed reusable component parameters, one preview/live renderer and budgeted relational rendering, not a claim that WordPress lacks dynamic bindings or templates.

The initial research's broad cluster-to-milestone assignment is refined to the agreed roadmap: multilingual work M3, collaboration/approval later workflows, optional local AI/extensions and Elementor import mapping M8. No capability is deleted or declared verified by reassignment. Protected-content policies remain M5; conditional visibility is not authorization. Arbitrary executable themes/assets are not assumed by this safe M2 graph and need a documented M8 extension boundary.
