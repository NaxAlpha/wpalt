# wpalt

An owner-operated Rust CMS with a bundled admin panel, composable website builder and integrated publishing, business, membership, learning and commerce features. SQLite supports the single-process setup; PostgreSQL also supports verified multiple processes on one Unix host. Native operation requires no wpalt vendor account or runtime license check.

**Current status:** M1–M9, M3.5 and defined D01–D11 follow-ups are delivered. D12's offline/recovery implementation closes through [PR28](https://github.com/NaxAlpha/wpalt/pull/28), with final checks and release publication tracked there. D13–D30 and additional earlier feature breadth remain. This is a development system, not complete WordPress/plugin parity or a public production-support commitment.

[Full status and remaining work](docs/PROJECT_STATUS.md) · [Resume/setup guide](docs/RESUME.md) · [Detailed backlog](docs/deferred-features.md) · [132-group capability matrix](docs/feature-parity.json)

## What works

- **Publishing:** posts/pages, typed models/taxonomies/directories, private drafts, revisions/autosave, assigned review, scheduling, rich block authoring, media and moderated comments.
- **Design:** bundled calm administration; themes, reusable declarative compositions, dynamic bindings, grouped multilingual navigation, local fonts and reviewed selected Elementor design imports.
- **Discovery:** translated publications, metadata/canonical/hreflang, typed visible structured facts/locations, discovery previews, image/video/news sitemaps, redirects and bounded editorial/link analysis.
- **Business:** composable forms/conditions/calculations, immutable survey cohorts, private attachments, consented audiences/mail and local analytics/engagement. Optional owner-hosted scanning and printable/PDF server attestations have explicit limits.
- **Membership and commerce:** restricted content/downloads, entitlements/groups/courses/quizzes/assignments/certificates, local catalog/orders/stock/receipts, purchase-derived access and reservations. External card processing and provider identity are optional connectors.
- **Offline/recovery:** deliberate public-text/scalar-form device copies, disconnected draft editing and explicit online review; separately requested/address-confirmed partial responses and a single purpose-specific reminder. No covert abandonment capture or automatic offline submission.
- **Operations:** CLI/TOML/environment configuration, redacted diagnostics, bounded jobs, encrypted/incremental recovery, native PostgreSQL PITR, selected migration, scoped integrations and coordinated same-host app/worker processes.

Every area has defined contracts, verification evidence and remaining limits. See the [full status](docs/PROJECT_STATUS.md) before treating a feature-group name as exhaustive parity. Arbitrary WordPress PHP plugins/themes do not execute unchanged. Cross-host distribution/failover and full localization/browser/accessibility coverage remain future work. Public licensing/support policy is not yet selected.

## Run from source

Rust 1.88 or newer is the declared CI-tested floor. Python 3 prepares a pinned build dependency; Node/npm builds frontend assets. The compiled application requires no Cargo, Node or PHP runtime.

```sh
python3 scripts/prepare_encoder.py
npm ci --prefix frontend --ignore-scripts
npm --prefix frontend run build
cargo build --release --locked
cp wpalt.example.toml wpalt.local.toml
./target/release/wpalt --config wpalt.local.toml init --admin-email you@example.com
./target/release/wpalt --config wpalt.local.toml seed-demo
./target/release/wpalt --config wpalt.local.toml serve
```

Initialization reads the password from stdin; use a private pipe/password manager because interactive input is not masked. Sign in at the configured origin's `/login`, default `http://127.0.0.1:3000`. Review [operations](docs/operations.md), [recovery](docs/resilient-operations.md) and [configuration example](wpalt.example.toml) before exposing meaningful data. Non-local sites require an HTTPS origin behind an owner-managed TLS reverse proxy.

Configuration precedence is CLI > supported environment variables > explicitly selected TOML > defaults. `wpalt config` shows validated redacted infrastructure settings; administration controls site content/design. Optional scanner/PDF/video/model tools and configured external services have their own declared host requirements. The executable is not a bundled PostgreSQL/scanner/browser installation.

## Verify and develop

`cargo test --locked` exercises SQLite; set a disposable `TEST_DATABASE_URL` and `WPALT_REQUIRE_POSTGRES=1` for required real PostgreSQL scenarios. Tests create/drop isolated schemas. Do not use a production database. Formatting, warnings-denied Clippy, pipeline safety, guidance freshness, real browsers, original-source upgrades/recovery and current dependency auditing remain cumulative gates.

Seven required CI jobs verify application, compiler floor, dependencies, a completely fresh release build, frontend, native PITR and local processes. Each successful main push produces a checksummed Linux x86_64/glibc development prerelease; a failed gate publishes nothing. This does not deploy or automatically upgrade databases. See [automatic releases](docs/automatic-releases.md) and the [current workflow](.github/workflows/m1.yml).

[Frontend measurement/design protocol](docs/frontend-foundations.md) · [Calm design](docs/calm-admin-design.md) · [Readable acceptance scenarios](tests/acceptance.rs) · [Browser runner](scripts/browser_acceptance.cjs)

## Guides and project contract

[Authoring](docs/authoring.md) · [Themes](docs/theme-authoring.md) · [Discovery](docs/discovery.md) · [Business](docs/business.md) · [Advanced forms](docs/advanced-forms-and-surveys.md) · [Private documents](docs/private-documents.md) · [Offline/recovery](docs/offline-and-recovery.md) · [Membership/learning](docs/membership-learning.md) · [Commerce/reservations](docs/commerce-reservations.md) · [Migration/integrations](docs/migration-and-extensions.md) · [Same-host processes](docs/distributed-operations.md)

[Product goals](docs/wpalt-product-goals.md) · [Development methodology](docs/wpalt-development-methodology.md) · [Milestones](docs/wpalt-milestones.md) · [Current guidance records](docs/evidence/feature-guidance.json) · [Full project status](docs/PROJECT_STATUS.md)
