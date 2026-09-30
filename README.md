# wpalt

An owner-controlled Rust CMS with a bundled admin panel and public website. M1 delivers a usable publishing system on SQLite or PostgreSQL, with no vendor account or external runtime services.

Create posts and pages, keep drafts separate from live content, autosave, preview, restore revisions and schedule publication. Manage images, typed fields, basic compositions, navigation, Paper/Ink themes, moderated comments and local search. Export content, download a consistent manual backup and recover into a fresh database, including across database engines.

![M1 administration panel](docs/evidence/screenshots/admin-desktop.png)

## Run

Rust 1.85 or newer is the declared build floor, verified by CI. Build once; the resulting server requires no Cargo, Node or PHP at runtime.

```sh
cargo build --release --locked
cp wpalt.example.toml wpalt.local.toml
./target/release/wpalt --config wpalt.local.toml init --admin-email you@example.com
./target/release/wpalt --config wpalt.local.toml seed-demo
./target/release/wpalt --config wpalt.local.toml serve
```

Initialization reads the password from stdin. Use a private pipe/password manager; the first CLI does not mask interactive input. Visit the configured origin's `/login` (default `http://127.0.0.1:3000`). Review [operations and recovery](docs/operations.md) before exposing a site. Non-local deployments require an HTTPS origin behind a TLS reverse proxy.

Configuration follows CLI > supported environment variables > explicitly selected TOML > defaults. The admin panel controls site content/design. `wpalt config` reports validated, redacted infrastructure configuration.

[Automatic clean post-merge builds and downloads](docs/operations.md#automatic-clean-builds-after-merge) are documented in operations.

## Review M1

- [Contract and deployment boundaries](docs/m1-contract.md)
- [Verification, performance, security and reference evidence](docs/evidence/m1-verification.md)
- [Readable acceptance journeys](tests/acceptance.rs), [real browser workflow](scripts/browser_acceptance.cjs), [CLI recovery journey](scripts/cli_acceptance.py)
- [Architecture decision](docs/decisions/0001-m1-architecture.md)

`cargo test --locked` always exercises SQLite. Set `TEST_DATABASE_URL` to an isolated PostgreSQL database to run both engines; CI requires it. Tests create/drop temporary PostgreSQL schemas. Use disposable databases, never production credentials.

## Project contract

- [Product goals](docs/wpalt-product-goals.md)
- [Development methodology](docs/wpalt-development-methodology.md)
- [Milestones](docs/wpalt-milestones.md)
- [132 researched capability groups and remaining milestone assignments](docs/feature-parity.json)
- [Current feature guidance](docs/evidence/feature-guidance.json) and [freshness gate](scripts/check_protocols.py)

M1–M8 are pre-adoption development. M1 provides basic themes/composition; the advanced builder is M2. Full WordPress/plugin parity and multi-server operation remain later milestones. M1 operates one application process per site/data directory, with manual unencrypted backups and fresh-target restore. Licensing is not yet selected. No WordPress or plugin implementation code is incorporated.
