# 0009 — Local recovery and account protection

Implementation decision, 2026-10-04; M7 remains in progress.

Use authenticated owner-held encrypted recovery packages and independently verified copies. Optional content-addressed storage keeps independently complete manifests; explicit bounded preview/hash pruning preserves retained points. Portable validation/file extraction work without a database or site installation. Full graph restore remains empty-target only; explicit M6 archive conversion is an offline migration utility, not a legacy runtime.

Use interoperable local TOTP and maintained webauthn-rs 0.5.5 passkey verification rather than implementing authenticator signature validation ourselves. Upgrade the Rust minimum to 1.88, required by this library. CI's compiler-floor gate follows that minimum. OpenSSL is vendored and statically built for self-contained release distribution; native build tools are needed only at build time. Lockfile and dependency audit cover the added dependencies. The new floor also enables standard let chains; denied-warning Clippy applied equivalent simplifications in existing validators without changing their decisions.

Schema 10 stores factors, one-use recovery hashes and public passkey credentials. Both belong in consistent recovery graphs. Keep server challenges ephemeral and private, user verification mandatory, credential IDs globally unique and concurrent credential updates checked against the current version. Password/factor changes and session revocation must beat stale enrollment or login. OIDC does not bypass a locally required TOTP factor.

Do not conflate successful local journeys with all of M7: native WAL recovery, remaining media workers/cleanup, abuse/privacy refinements and integrated milestone evidence remain visible in the contract and matrix until implemented and verified.

2026-10-04 follow-up: archive v10 includes the recent 200 validated operational audit records. HTTP and privileged CLI intents/outcomes share a bounded journal; recent inspection spans rotation, deduplicates and orders timestamps. Unreleased v9 previews restore with their matching executable before upgrade/re-export, or reset disposable fixtures. No v9 parser was added to ordinary restore. The explicit M6 converter parses byte vectors directly rather than building a per-byte generic JSON tree.
