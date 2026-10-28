# Resume development

Updated 2026-10-28 (Asia/Tokyo). Read [full project status](PROJECT_STATUS.md), `AGENTS.md`, product goals, methodology, milestone roadmap and [remaining backlog](deferred-features.md) first. The old October 17 checkpoint is retained in [its exact-source receipt](evidence/d12-checkpoint.json) and source-only tag `checkpoint/d12-tested-20261017`; it predates the closure dependency fixes.

D12's defined implementation closes through [PR28](https://github.com/NaxAlpha/wpalt/pull/28). Check its merge/main/release result before starting D13. The newer closure updates security dependencies after the old checkpoint's audit became outdated. Historical green checks or metrics do not replace current dependency auditing.

```sh
git clone https://github.com/NaxAlpha/wpalt.git
cd wpalt
python3 scripts/prepare_encoder.py
npm ci --prefix frontend --ignore-scripts
npm --prefix frontend run build
cargo build --locked
```

The [README](../README.md) explains initialization/configuration and test setup. Use disposable PostgreSQL 17 plus `TEST_DATABASE_URL`/`WPALT_REQUIRE_POSTGRES=1` for the cumulative SQLite/PostgreSQL suite. The committed workflow is the canonical seven-gate browser/scanner/PDF/encoder/source-recovery recipe. Runtime binaries, caches, local databases and old absolute socket paths are not required to resume source development. Optional tools are recreated from pinned preparation scripts/workflow sources.

D13 is preparation only: [authority assessment](evidence/d13-preparation/authorization-assessment.md), [lexical inventory](evidence/d13-preparation/permission-callsite-inventory.json), [free role-editor source provenance](evidence/d13-preparation/free-role-editor-source.json). Reassess current guidance and specify a concrete substantial user-facing custom authorization system. Separate learning/commerce delegation; enforce direct API/download/content/worker authority and mutation-boundary revocation; do not invent owner power from a custom role or grant integration/recovery credentials implicitly. The lexical inventory is not semantic coverage proof. Password/passkey/provider/CLI session constructors require review.

Start D13 on a separate branch/PR after D12 release verification. The owner authorizes feasible local work and ready-PR merges; genuine external accounts/keys/infrastructure and owner-only adoption decisions require their input. Each next package needs functional journeys, optimization/security/recovery/UI evidence and the exact-source CI/main/public-archive process. Keep earlier retained breadth visible; do not confuse defined packages with full plugin parity.

The [D12 review packet](evidence/d12-review/index.html) and compact evidence are committed. Ignored random test credentials/databases, raw old logs/coverage counters, downloaded models/tools and local macOS executables are not in Git. Tests recreate synthetic sites, not the identical historical local fixture. Preserve independently valuable private site data separately; never publish it for a development checkpoint. Stopped explicit native 25 / portable 20 maintenance is documented in [offline/recovery](offline-and-recovery.md); original public releases and their original verifiers support authentic prior-source recovery without the old workspace.
