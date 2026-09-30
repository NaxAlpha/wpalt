# M1 verification and review evidence

Date: 2026-09-30. Scope: [M1 contract](../m1-contract.md). Local verification is complete; independent Linux/compiler-floor CI is being finalized on [delivery PR #1](https://github.com/NaxAlpha/wpalt/pull/1). This is a publishing CMS milestone, not final WordPress/plugin parity or a production-support release.

## Review the end-user system

[Admin desktop](screenshots/admin-desktop.png) · [Public desktop](screenshots/public-desktop.png) · [Public mobile](screenshots/public-mobile.png) · [Editor mobile](screenshots/editor-mobile.png)

Build/initialize/seed/serve using the README, then follow these review journeys:

1. Create a page/post, categorize it, supply fields/composition, preview privately and publish. Edit/autosave the working copy; the visitor still sees the previous live title/body/URL/terms. Restore a revision and deliberately publish the restored work.
2. Schedule a draft, stop/restart before its deadline and observe one publication. Scheduling a previously published item intentionally hides it until due.
3. Upload a private image and verify an unauthenticated visitor cannot read it; change visibility and description. Submit a public comment, approve it through moderation, and check the public page.
4. Switch Paper/Ink and navigation. Search published content, paginate the library and read RSS. Create editor/moderator accounts, observe permission boundaries, then disable an account and verify existing sessions are revoked.
5. Download a full snapshot, stop the site, restore to a fresh database/data directory and sign in again. Restore can cross SQLite/PostgreSQL. A nonempty target and corrupted/path-unsafe archives fail.

## Behavioral evidence

Eight readable integrated Rust journeys exercise real SQLite and PostgreSQL, using temporary schemas for isolation. They cover live/draft confidentiality, optimistic concurrent saves, persisted/idempotent schedules, role/Origin/CSRF/session boundaries (including expiry, disable/revocation and last-administrator protection), validated/private images, moderation, cross-engine recovery/corruption, stable pagination/live-only search and invalid configuration/composition. The concurrent-edit journey requires one save to succeed and the stale save to return conflict, then checks the resulting version/revision state.

CLI acceptance uses actual release processes and HTTP, verifies configuration precedence and exclusive process locking, restarts the scheduler, parses RSS as XML, creates a new private backup file, recovers a fresh site and scans debug logs for generated passwords/session/CSRF secrets. Browser acceptance uses Chrome/Playwright with owner and visitor contexts, exercises autosave/revision restore/media/moderation/theme switch, reviews 1440-pixel desktop and 390-pixel mobile layouts and verifies no horizontal overflow. [Browser result](browser-result.json): no external runtime requests or JavaScript exceptions in this journey. This does not certify universal browser compatibility or WCAG conformance.

Formatting, strict Clippy, locked release build, evidence freshness/compatibility gates and the selected dependency graph audit pass locally. GitHub CI repeats dual-engine behavior, CLI/browser journeys, dependency audit and a separate declared Rust 1.85 compiler-floor check. CI results are authoritative on the PR, not inferred from local macOS results.

## Performance and query evidence

[Raw final baseline](performance.json) includes p50/p95/p99, throughput and response bytes for 18 scenarios: home/story/search on both databases plus WordPress, 200 requests per scenario, concurrency 1 and 10. wpalt ran as a native optimized arm64 binary on macOS 26.6.2 with 8 logical CPUs. PostgreSQL 17.11, WordPress 7.1.2/PHP 8.3 and MariaDB 11 ran together in a Podman Linux VM with 4 CPUs/2 GiB. Requests measure complete uncompressed HTML through a Python localhost client after warm-up; they exclude browser assets/rendering. There is no application response cache.

| System | Home p95, c1 / c10 (ms) | Story p95, c1 / c10 (ms) | Search p95, c1 / c10 (ms) |
|---|---:|---:|---:|
| wpalt / SQLite | 0.550 / 3.216 | 0.484 / 2.715 | 2.831 / 27.539 |
| wpalt / PostgreSQL | 1.515 / 7.093 | 2.301 / 11.101 | 2.954 / 10.372 |
| WordPress reference / MariaDB | 41.131 / 144.594 | 48.812 / 170.472 | 45.345 / 175.643 |

These are fixture observations, not a fair hardware-identical contest or proof of full-platform superiority. HTML/content/plugin scopes differ; VM/network/client overhead and shared development-host activity affect measurements. WordPress has 1,002 published posts versus wpalt's 1,000 stories plus About. Do not calculate universal speed/memory ratios from these runs.

The [initial run](performance-before.json) exposed PostgreSQL search p95 44.615/145.371 ms immediately after bulk writes. Its later plan used the GIN index and became fast after recorded autovacuum/autoanalyze. GIN pending-list maintenance is a supported explanation, not directly instrumented causal proof. Offline seed/restore now explicitly perform bounded post-commit planner/index maintenance; the fresh final fixture above verifies the resulting behavior. Routine autovacuum remains necessary.

[SQLite plans](query-plans-sqlite.txt) and [PostgreSQL plans](query-plans-postgres.txt), captured on 1,001-item fixtures using [the reproduction example](../../examples/query_plans.rs), show indexed keyset pagination, schedule selection, primary-key session lookup and FTS5/GIN search. Search still sorts matching hits; it is not constant-cost for arbitrary corpora. SQLite chooses a scan of the one-row users table after its session index lookup; no claim that every tiny-table scan is wasteful. PostgreSQL search's recorded execution is 1.437 ms with 1,000 matching hits. Plans use representative literals of the application's bound query shapes; empty schedule/session fixtures do not measure occupied queue/login load.

[Authoring diagnostics](diagnostics.json) use the release with debug SQL enabled: admin content-list p95 0.755 ms and editor-read p95 0.640 ms across 50 reads. Correlated SQL counts are home/search 2, story 4, library 3, editor 5, without per-item query expansion. This measures HTTP reads, not typing/render latency; the browser journey verifies actual autosave feedback and interaction. Debug search p95 19.438 ms illustrates why release/non-debug and debug measurements must stay distinct.

The executable is approximately 9.44 MiB, with bundled CSS/JS and no runtime Node/PHP dependency. Native wpalt RSS after public load was 45.77 MiB SQLite / 31.25 MiB PostgreSQL; external database memory is excluded. Separate debug/authoring diagnostics observed idle initialized RSS 30.80 MiB and active-after-read RSS 36.00 MiB. SQLite fixture database/WAL footprint was 12.93 MiB (no uploaded images). PostgreSQL public table/index total was 8,757,248 bytes, excluding global DB/server/VM overhead. WordPress core/theme/plugin source tree was 164,048 KiB and its MariaDB table/index total 2,981,888 bytes; these are different storage boundaries and include neither complete installation dependencies nor image data.

M1 reference regression budgets, requiring the same fixture/environment and a documented rerun rather than flaky correctness assertions: non-debug warm p95 <10 ms at c1 and <50 ms at c10 for the measured public routes; native app RSS <80 MiB excluding external databases; release binary <15 MiB; SQLite fixture files <25 MiB excluding media/logs/build caches. They are review targets for this dataset, not guarantees on every machine/configuration. Explain changes before adjusting budgets.

Reproduce with `scripts/benchmark.py` (release binary; optional PostgreSQL URL must be an empty isolated database), `scripts/diagnostics.py` (macOS RSS tooling), and `DATABASE_URL=… cargo run --example query_plans`. Raw benchmark conditions and variants are retained so future comparisons cannot silently change their meaning.

## Security and failure boundaries

Passwords use Argon2id; session identifiers are random and stored by digest; cookies are HttpOnly/SameSite and Secure for HTTPS. State changes require matching Origin and authenticated writes require CSRF. Roles are checked server-side. Disable/password/role changes revoke sessions, with last-admin protection. Bound SQL values, escaped templates, sanitized Markdown, strict CSP and allowlisted bounded image decoding protect major input boundaries. Optimistic versions prevent silent lost updates; transactions couple content, revisions and terms. Request/worker/password work, pool waits, body size, statement duration, revisions and snapshot size are bounded. SQL/request diagnostics correlate IDs while excluding bound values, cookies, passwords and content.

[Dependency audit](dependency-audit.json) checked current RustSec evidence against `cargo tree --target all --edges normal,build,dev`: zero active vulnerabilities or warnings. The lockfile contains inactive optional `rsa 0.9.10` / RUSTSEC-2023-0071 from disabled SQLx MySQL support; it is recorded rather than broadly ignored. The gate fails if it becomes active. An advisory scan and these tests do not prove absence of security defects; future review/fuzzing remains valuable.

M1 is single-process coordination. Backup checksums detect corruption, not malicious replacement; archives contain private data/password hashes and are unencrypted. Restore trusts only owner-selected archives and requires empty targets. Failed file staging can leave unused UUID media but cannot partially commit database contents. TLS terminates at the operator's proxy. Forwarded client IP headers are not trusted; a proxy shares the direct-peer comment throttle. Tests simulate restarts and invalid archives, not power-loss/fsync correctness across all storage hardware, hostile distributed writers, or a complete penetration test.

## WordPress reference and ongoing protocols

The disposable reference used official container images and installed free ACF 6.8.10 and Admin and Site Enhancements 9.1.4. [Recorded reference](wordpress-reference.json) verifies WordPress draft/publish/revision APIs and ACF text/boolean fields, then seeds comparable publishing/search content. ACF field definitions were registered through a local PHP reference script; no claim of complete plugin GUI or paid-feature implementation inspection. WordPress `wp-includes/post.php` was inspected for reference. No implementation code was copied into wpalt.

Feature guidance records map primary sources, applicable versions, review dates, owners, requirements and tests. Security records are due 2026-10-30, normal records 2026-12-29; release CI rejects overdue evidence and overdue compatibility bridges. Maintainers must read changed sources, assess applicability and update behavior/tests/evidence—changing a date alone is insufficient. This is an owner-operated release protocol, not runtime vendor-account enforcement or automatic certification. Full SEO is M3; M1 basic titles/metadata do not imply SEO parity.

The compatibility register is empty. Schema mismatches fail explicitly; no obsolete API/theme/runtime compatibility is promised before M9. The capability matrix keeps all 132 researched groups visible for later milestone reconciliation.
