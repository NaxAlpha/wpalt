# M1 contract — usable publishing CMS

Status: M1 implementation and verification complete; [delivery PR #1](https://github.com/NaxAlpha/wpalt/pull/1) was merged following user approval on 2026-10-01 (Tokyo). Current-head CI is a gate on that PR.

M1 delivers an installable Rust application with bundled admin/public assets. A site owner can run a content website on a local machine or single server, using SQLite or PostgreSQL, without a vendor account. This contract refines M1 in the agreed roadmap; it does not claim final WordPress/plugin parity.

## Reviewable outcomes

| ID | Outcome and completion evidence |
|---|---|
| M1-01 | Install/configure through TOML, environment and CLI; initialize administrator securely; inspect redacted effective configuration. CLI acceptance verifies precedence, invalid settings, initialization and locking. |
| M1-02 | Authenticate, create collaborators and enforce administrator/editor/moderator boundaries, session expiry/logout, CSRF and same-origin mutations. HTTP journeys verify both allowed and denied operations. |
| M1-03 | Create posts/pages with categories/tags, typed fields and basic composition; draft, autosave, preview, publish, unpublish and restore revisions without leaking working copies. Acceptance checks actual public output and search. |
| M1-04 | Schedule publication durably; reopen the application and publish once when due. Verify no premature visibility or duplicate publication revision. |
| M1-05 | Manage image uploads, descriptions and public/editor-only visibility; reject unsupported, malformed and resource-excessive uploads. Verify HTTP access boundaries and restored bytes. |
| M1-06 | Run a public website with basic Paper/Ink themes, navigation, search, cursor pagination and RSS. Validate browser rendering and XML, not only successful HTTP status. |
| M1-07 | Accept public comments into moderation; approve/reject/hold; escape output and limit repeated submissions. Verify public visibility before/after moderation. |
| M1-08 | Export portable content and create consistent manual database/media backups; recover into a fresh site, including cross-database restore, without sessions or vendor login. Reject corruption, unsafe paths and nonempty targets. |
| M1-09 | Observe correlated requests, timings, query diagnostics and security events without logging credentials/content. Verify representative logs, resource measurements and query plans. |

Every applicable journey runs against real SQLite and PostgreSQL. Meaningful component validation is exercised alongside these integrated journeys; avoid duplicate trivial assertions.

## Deployment boundary

One application process owns a data directory. An OS advisory lock prevents simultaneous CLI/server use of that directory and is released on process exit. Admin backup capture is supported while serving because it uses the same mutation coordinator as content/media changes. CLI backup/restore require the server stopped. Independently running clients must not mutate the database/media behind wpalt; multi-process coordination is M9.

SQLite uses WAL, foreign keys, a busy timeout, indexed listing/scheduling and FTS5. PostgreSQL uses indexed listing/scheduling, a GIN text-search index, bounded statement time and repeatable-read logical backups. SQL parameters remain bound; only trusted SQL fragments enter builders.

Non-local sites require an HTTPS public origin behind a TLS reverse proxy. M1 serves HTTP on a configurable listen address; it does not bundle certificate automation. Configure the proxy with matching origin, request-size/time limits and suitable trusted network exposure. Client-forwarded IP headers are not trusted; comment rate limiting uses the direct peer, so a reverse proxy shares that limit unless operated accordingly.

## Explicit initial limits

- Markdown editor and simple text/heading/callout compositions; the full visual theme builder is M2.
- Posts/pages, fixed initial roles and primitive typed fields; relations/repeaters, granular policies and richer content models arrive in later milestones.
- ASCII URL slugs; manual translations/multilingual discovery and comprehensive SEO are M3.
- Images only (PNG/JPEG/WebP/GIF), one file per upload, at most the configured bytes and 4096 pixels per dimension/64 MiB decoding budget. General document uploads and advanced image optimization are later work.
- Comments display the first 100 approved entries; media/users/moderation initially show bounded recent lists. Public and content-library pagination are cursor-based. Later library tooling expands discovery and filtering.
- Scheduling a published item removes it from public view until due; saving/autosaving a published item's working copy leaves its public snapshot unchanged.
- Backups are checksum-verified, unencrypted JSON with password hashes/private data. Store them securely and separately from the origin host. Configured size limits bound backup work; this is not the incremental/encrypted engine of M7.
- Restore only into an empty target. A failed restore may leave unused UUID-named media, but commits no partial database site; retry can replace those validated files. No destructive in-place upgrade compatibility is promised before M9.
- Content JSON export omits user credentials and is separate from full recovery. Import is via the complete snapshot restore in M1; broad WordPress migration is M8.

## Optimization and release evidence

First establish correct behavior, then measure and optimize the integrated system. Record release-build measurements on a declared machine, dataset and concurrency level, including p50/p95/p99, throughput, memory, binary/assets and data footprint. Compare to the recorded WordPress reference with equivalent scope where possible; do not extrapolate M1 numbers to the final platform.

Record query plans for public pagination, scheduled publication, session lookup and search for both engines. Publish the measured baseline and justified M1 regression budgets in `docs/evidence/m1-verification.md`. No timing assertions belong in ordinary correctness tests.

Completion requires formatting/lint/build checks, dual-database behavioral results, a browser review, CLI/restart/recovery evidence, relevant security/dependency review, recorded measurements, current guidance checks and the delivery PR. Known limitations must remain visible in that PR.
