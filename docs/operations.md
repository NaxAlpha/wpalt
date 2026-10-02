# Running and recovering wpalt

## Build and initialize

Use Rust and Cargo. Build once; runtime does not need Cargo, Node, PHP or internet access:

```sh
cargo build --release --locked
cp wpalt.example.toml wpalt.local.toml
./target/release/wpalt --config wpalt.local.toml config
./target/release/wpalt --config wpalt.local.toml init --admin-email you@example.com
```

`init` reads a password of 12–256 bytes from stdin. Supply it through your password manager or a private input file/pipe; do not put a real password in shell arguments or committed scripts. Interactive stdin is not masked by this first CLI, so prefer a secure pipe. The CLI prints no password. For review fixtures, `seed-demo` creates published stories and an About page on an initialized empty library.

```sh
./target/release/wpalt --config wpalt.local.toml seed-demo
./target/release/wpalt --config wpalt.local.toml serve
```

Visit the configured origin's `/login`. Keep the working directory and configuration consistent across commands; relative paths are relative to the process working directory. Files in `data/`, `work/` and `wpalt.local.toml` are ignored by Git.

## Configuration

Precedence: explicit CLI flags > supported `WPALT_*` environment variables > the selected TOML file > defaults. There is no implicit config-file discovery. Unknown TOML keys and invalid ranges fail. Settings in the admin panel govern site content/design; CLI/TOML govern runtime and infrastructure. Runtime changes require restart in M1.

Environment keys: `WPALT_DATABASE_URL`, `WPALT_DATA_DIR`, `WPALT_LISTEN`, `WPALT_BASE_URL`, `WPALT_DEBUG`. Other settings use TOML. Credentials in PostgreSQL URLs are redacted by `config` and the operations panel.

Choose a matching database path and data directory when overriding defaults. PostgreSQL example: set `WPALT_DATABASE_URL` from a secret manager to a valid PostgreSQL connection URL, select a private `data_dir`, then initialize. Use certificate-verifying database TLS for connections outside a trusted local environment. Do not commit database credentials.

Expose non-local sites through a TLS reverse proxy and set `base_url` to the exact public HTTPS origin. Secure cookies then activate; mutation Origin checks expect that origin. Do not rewrite Origin headers or forward untrusted host/IP information as authority. Add proxy timeouts and request-size limits. The application does not fetch outside services during ordinary M1 use.

## Debug and diagnostics

Use `--debug` or `debug = true` for SQL statement/timing diagnostics. Requests have server-generated correlation IDs in `x-request-id` and structured JSON logs, with method, route template, status and timing. Security events cover denied access, failed/successful login and relevant state changes. SQL values, cookies, CSRF tokens, passwords, content bodies and query strings are not logged. SQL diagnostics report statement structure rather than bound values. Logs go to stderr; use owner-controlled rotation, retention and access policies.

`/health` verifies an initialized site through the database. It is not an independent uptime monitor. The operations panel shows effective redacted configuration and recovery controls.

## Manual backup and fresh restore

The admin panel can download a snapshot while the application runs. Protect that download: it includes private content and password hashes, and M1 does not encrypt it. For CLI backup, stop the server first:

```sh
./target/release/wpalt --config wpalt.local.toml backup /secure/location/site-backup.json
```

The destination must be a new file; Unix permissions are restricted. Keep an independent copy, with credentials outside the site's failure domain when appropriate. SHA-256 detects corruption, not authenticity against an attacker who can rewrite the archive; restore only trusted backups.

To recover, use a fresh database and data directory in a new configuration, then:

```sh
./target/release/wpalt --config recovery.toml restore /secure/location/site-backup.json
./target/release/wpalt --config recovery.toml serve
```

Do not run `init` first: restore requires an empty target. A snapshot can move between SQLite and PostgreSQL. Restored sessions are invalidated; use the original account credentials to sign in. The target runtime/database configuration is retained, not taken from the archive. Test recovery before relying on a backup.

Backups include site settings, users, working/published content, revisions, taxonomies, media and comments; derived search indexes rebuild through insertion. They exclude sessions, rate-limit state, runtime secrets, binaries and external integrations. Content export in the admin panel is a separate credential-free content JSON file, not a complete recovery artifact.

## Development upgrades

M1–M8 have no public production support promise. Keep meaningful data backed up. A breaking change must document migration or reset/reseed of disposable fixtures. Unknown schema versions are rejected rather than silently interpreted through old code. M9 establishes the supported public upgrade lifecycle.

## Bulk-operation maintenance

Offline demo seeding and restore run PostgreSQL `VACUUM (ANALYZE) posts` or SQLite `PRAGMA optimize` after committing content. PostgreSQL autovacuum remains enabled; operators should monitor it normally. Failure to run this maintenance is logged as a warning because imported data has already committed. Retry maintenance through your database tooling; do not repeat a completed restore into a populated target. The database statement timeout also bounds this maintenance.

See [PostgreSQL GIN behavior](https://www.postgresql.org/docs/17/gin.html), [VACUUM](https://www.postgresql.org/docs/17/sql-vacuum.html) and [SQLite PRAGMA optimize](https://www.sqlite.org/pragma.html#pragma_optimize).

## Automatic clean builds after merge

Each push to `main`, including a merged PR, runs application/dual-database/browser/security checks and the compiler-floor check. Only when both jobs pass does `clean-build` compile the exact pushed commit on a new Ubuntu 24.04 runner, using empty Cargo/target directories without restored dependency/build caches. The build uses locked dependencies and the recorded Rust 1.98.1 compiler. Earlier PR builds do not substitute for this post-merge verification. Main runs are grouped by commit so a later merge does not cancel an earlier merged commit's build; updated PR runs can still replace old PR runs.

Open the successful **Application verification and builds** run under GitHub Actions and download `wpalt-clean-linux-x86_64-<commit>`. It contains a tar.gz preserving executable permissions, `SHA256SUMS`, the bundled server, example config, operations notes and `BUILD.json` identifying source/toolchain/lock/binary hashes. Artifacts are retained for 90 days (subject to repository policy); this is development distribution, not automatic versioned GitHub Releases. Verify the archive checksum before extraction. The binary targets Linux x86_64/glibc; other target archives can be added with their own verification.

“Clean” means no restored compilation/dependency cache or old workspace artifacts. It does not claim byte-identical reproducible builds across toolchains/hosts. A failed check/build produces a failed run rather than a successful clean artifact.

Protocol checked 2026-10-01 against [GitHub push events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#push), [job dependencies](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds) and [artifact downloads](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts).

## M1 to M2 data upgrade

Stop the server and take an independent M1 backup with the M1 executable before replacing it. M2 automatically performs a one-off transactional schema-1 to schema-2 migration on startup/offline commands: working/live content, users, media, terms and revisions remain, typed definitions are converted, themes/options/models are initialized, and SQLite foreign keys/search are checked. PostgreSQL constraints are updated transactionally. After success, only schema 2 is used by ordinary requests. Test the upgrade on a restored copy before applying it to meaningful data.

M2 snapshots use `wpalt-backup-v2` and include models, shared draft/live options, themes and revision histories. Restore into an empty database/data directory only. M1 snapshots must first be restored with M1, then opened by M2. A downgrade is restoration of the independent M1 backup into a fresh target, not opening a schema-2 database with M1. Only explicitly disposable fixtures may be reset. No old-format runtime compatibility parser was added.

Theme package import/export/publication/activation also works through the offline CLI; see [authoring guide](theme-authoring.md). Node/npm are needed to rebuild studio assets during development, never to run a packaged server. Both normal verification and uncached builds regenerate locked frontend assets and reject uncommitted differences before packaging. Packages preserve the binary's executable bit and commit/toolchain/checksum manifest.

## M3 discovery upgrade

See [discovery operations](discovery.md#upgrade-and-recovery) for schema-3 migration, backup-v3 restoration, old-archive handling and current content API metadata requirements. M3 uses no mandatory cloud service or account.
