# ADR 0001 — a small single-process publishing system

Status: accepted for M1; reassess at milestone preparation. Date: 2026-09-30.

## Decision

Use Rust/Axum/Tokio, SQLx with SQLite/PostgreSQL drivers, Maud server-rendered escaped templates, bundled CSS and a small progressive-enhancement script. Store structured content and compositions as validated JSON, with relational ownership, taxonomy and revision records. Markdown rendering is sanitized. No third-party plugin/theme implementation is copied.

## Why

A bundled server/UI provides a usable installation without a JavaScript build chain or vendor runtime. Progressive enhancement supports autosave while ordinary forms remain usable. M2 can introduce richer interactive authoring without preserving this initial editor's internals. Database capabilities are adapted explicitly rather than pretending SQLite and PostgreSQL are interchangeable distributed engines.

SQLx 0.8.6 is the locked, tested baseline for the declared Rust 1.85 dependency floor; local compilation uses Rust 1.96 and independent CI verifies Rust 1.85. SQLx 0.9 requires a newer compiler. Dependency/security checks and guidance records govern later updates. The compiler-floor CI check is a required release gate.

## Integrity and concurrency

One process holds an advisory data-directory lock. Mutations share an async coordinator; saves also carry optimistic version checks and transactional revisions/taxonomies. Public snapshots are separate from working content, including slug and terms. Scheduling persists in the database rather than a page-load trigger. Backups coordinate with writes and take a consistent logical snapshot; sessions are omitted from recovery.

This coordinator is deliberate M1 single-server behavior, not a distributed lock. M9 replaces/extends coordination based on its documented topology and invariants.

## Query design

Use bound values and static SQL fragments. Static queries use numbered placeholders supported by both engines. SQLx `QueryBuilder<Any>` emits question marks; a narrow database boundary numbers these placeholders while retaining separate arguments. This transformation is only for trusted SQL without literal question marks. Dual-engine journeys caught and verified this distinction.

Public lists select summaries, never drafts or complete body blobs, and use keyset pagination. SQLite FTS5 and PostgreSQL GIN/full-text queries avoid wildcard scans. Published-search indexing does not change for an unchanged live snapshot during autosave. Indexes cover schedule, sessions, revisions, taxonomy joins and moderation. Snapshot rows are streamed and size-limited rather than fetched as one unbounded result.

## Operational tradeoffs

M1 has a simple editor, primitive fields and two built-in themes. Backups are bounded logical JSON and unencrypted. Image decode and password verification use bounded blocking workers. TLS is provided by an operator-configured reverse proxy. These limitations are explicit in the contract; full composition, advanced media, automated resilience and distributed execution remain their planned milestones.

No pre-M9 legacy API/config/theme compatibility layer is introduced. Schema version mismatches fail explicitly. Meaningful data needs a migration or deliberate preservation decision when the model changes.

## Compiler-floor verification correction

The first Rust 1.85 CI run found `yoke-derive 0.8.3` using `str::from_utf8`, unavailable on that compiler despite its parent dependency's older declared floor. The lockfile selects compatible `yoke-derive 0.8.2`, whose implementation uses `core::str::from_utf8`, while retaining `yoke 0.8.3`. Locked builds and the compiler-floor CI gate preserve this verified selection. Reassess with a future dependency upgrade rather than adding application compatibility code or assuming dependency metadata proves compiler support.
