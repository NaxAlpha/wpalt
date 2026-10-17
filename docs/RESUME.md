# Resume wpalt from the October 17 checkpoint

Saved 2026-10-17 (Asia/Tokyo) at the owner's request to retain intermediate work before removing the local workspace. This is a resumable development checkpoint, not a completed D12 release. Application work is in draft [PR28](https://github.com/NaxAlpha/wpalt/pull/28), branch `feature/d12-offline-recovery`.

## Start here on a fresh machine

```sh
git clone --branch feature/d12-offline-recovery https://github.com/NaxAlpha/wpalt.git
cd wpalt
```

Read this file, `AGENTS.md`, the product goals, development methodology, milestone roadmap and `docs/d12-contract.md`. `docs/deferred-features.md` and `docs/feature-parity.json` retain the complete remaining backlog. No local chat history, ignored work directory or previous absolute filesystem path is required to continue development.

## Exact saved implementation and evidence

D12 implementation head: `27c897a74832c284ef29aeb19f5d87dce1d11fdb`. GitHub tested merge: `c8f7a12bb0a648832114bd2543b42c42b45d0be3`. Both have Git tree `eb8fda1ebca833eddd6c37859f95e4c5f011ef8f` before this documentation-only checkpoint. The source-only tag `checkpoint/d12-tested-20261017` retains the tested merge independently of the moving PR ref. The following checkpoint commit adds documentation, evidence receipts, source-inspection metadata and actual synthetic review images; it does not change application behavior, dependencies, generated assets or workflows.

[Implementation CI 38316460776](https://github.com/NaxAlpha/wpalt/actions/runs/38316460776) completed successfully: application, clean-build, compiler-floor, dependency-audit, frontend, native-pitr and local-processes. The clean Linux archive was independently downloaded and passed the strict source/dependency/embedded-asset/guide/file-set/ELF/checksum verifier. See [checkpoint receipt](evidence/d12-checkpoint.json). This receipt belongs to the implementation source above; a newer documentation commit is not silently called the same tested source. GitHub Actions artifacts expire, so their URLs are historical evidence, not permanent runtime dependencies.

The cumulative suite has 148 Rust scenarios with required SQLite/PostgreSQL, 13 pipeline safety tests and real browser journeys. This is an inventory, not a coverage percentage. Local browser evidence uses Chrome 155 and a debug executable; exact identities, measurements and limits are in [D12 verification](evidence/d12-verification.md), [offline receipt](evidence/d12-local-browser.json) and [native-form receipt](evidence/d12-native-form-browser.json). The [small self-contained review packet](evidence/d12-review/index.html) preserves actual populated/mobile/failure screens without the original outputs folder.

## Finish D12 before implementing D13

1. Inspect PR28 and any newer changes. Preserve the explicit public-text/scalar-form offline boundary, original interrupted response identifiers and separate requested/address-confirmed recovery authority.
2. Refresh applicable guidance/dependency records when resuming. Do not simply change last-checked dates. Historical source reviews and historical tests retain their original dates/versions.
3. If source changes, rerun relevant local checks and all seven cumulative gates on the final intended merge. Independently download and strictly verify that final clean artifact. Match the tested merge tree to the intended tree before merging.
4. The owner has standing authorization to merge ready PRs. D12 remains a draft checkpoint for now; it has not been merged or released. Once ready, merge and verify all seven gates on actual main, then separately download and verify the public release. Record source/tree/run/tag/archive/binary identities, not just PR green status.
5. Update D12 delivery state in the ledger, operator guide, backlog and parity matrix. Only then start a separate D13 delivery branch/PR.

D11 remains the last delivered baseline: main `22e29f7d3a310f565a97107bbb5b5f328ec1d39b`, [original public release](https://github.com/NaxAlpha/wpalt/releases/tag/nightly-202610121346-22e29f7d3a31), [independent receipt](evidence/d11-main-release.json). D12 uses native schema 25/portable 20, explicit stopped maintenance and original D11 conversion/rollback. Ordinary requests/restores do not parse old formats. The cumulative workflow fetches original published Linux releases and original verifier sources; it does not depend on retained local macOS binaries or fixture databases.

## D13 preparation already done

There is no D13 application implementation yet. Read the [authorization assessment](evidence/d13-preparation/authorization-assessment.md), [lexical callsite inventory](evidence/d13-preparation/permission-callsite-inventory.json) and [free reference source provenance](evidence/d13-preparation/free-role-editor-source.json). The inventory is a discovery aid, not complete semantic authorization proof.

Separate learning/commerce delegation; fixed-role session checks, direct protected content/downloads, editorial reviewer/worker eligibility and integration credentials currently use different authority paths. Custom roles need a finite explicit registry, immutable owner-only powers, canonical persisted grants, consistent current permissions after mutation locking, prompt revocation and tested private recovery. Review all password/passkey/provider/CLI session constructors. A request-scoped actor recheck in the shared mutation coordinator is an unimplemented design candidate; inspect nested locking, anonymous requests, background jobs and scoped integration actors before choosing it.

Free User Role Editor 4.66.2 was source-inspected from its pinned 618,966-byte official archive. No plugin code was incorporated and no PHP/WordPress/premium runtime was executed. Its administrator-assignment protection across AJAX/classic screens is a useful adversarial case. Primary documentation/source inspection was checked 2026-10-13; reassess current guidance at D13 specification time.

D13–D30 and earlier retained breadth remain open; D17 actual payment-account certification, D28 physical-host infrastructure and D30 owner licensing/support decisions may need the owner. Continue feasible local work autonomously; request genuine accounts/keys/infrastructure only when required. Do not replace full-product scope with the defined slices.

## Recreate the development environment

Rust 1.88 is the declared CI compiler floor; the checkpoint was locally verified with Rust 1.96 and CI 1.98.1. Use the committed locks. Node/npm, Python 3, database client tools and browser tooling are development dependencies; the built application needs no Cargo/Node/PHP runtime.

```sh
python3 scripts/prepare_encoder.py
npm ci --prefix frontend --ignore-scripts
npm --prefix frontend run build
cargo build --locked
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
python3 scripts/check_protocols.py
python3 scripts/schema_vocabulary.py --check
python3 -m unittest discover -s tests/pipeline -v
```

Use disposable PostgreSQL 17 and set `TEST_DATABASE_URL` plus `WPALT_REQUIRE_POSTGRES=1` for `cargo test --locked`; the suite creates/drops isolated schemas and always exercises SQLite. Never point it at meaningful production data. PostgreSQL Unix-socket paths from old logs are local fixture details, not required configuration. The seven-job workflow is the canonical cumulative Linux recipe, including pinned scanner, encoder, browser/PDF, free-plugin reference and authentic migration fixtures.

Browser instructions and independent dimensions/baselines are in `docs/frontend-foundations.md`; runner modes are selected by `scripts/browser_acceptance.cjs`. Prepare the specified actual scanner/PDF fixture for private-document journeys. Browser, scanner, model downloads and original public reference runtimes are recreatable using scripts/workflow pins. Do not claim unavailable real-provider or paid-plugin verification.

## What deleting the old workspace discards

Application source, committed contracts/research, resume notes, D12 review images and compact verification receipts are pushed to GitHub. The secondary `build/encoder-bootstrap` branch at `4f9a004` was already pushed; its linked worktree is unnecessary for the active branch.

Ignored test databases, random fixture credentials, raw local logs, downloaded tools/models, Cargo caches, retained local executables, old raw coverage counters and other unversioned work are not stored in Git. They are not required to resume development or the canonical cumulative CI. A future test run creates fresh fixtures; it does not recover the exact old local test site. Historical coverage claims remain bound to their committed source/report, not current coverage. Back up any independently valuable private site data before deleting its files; never publish such data merely to retain a development checkpoint.

The October 17 intermediate cleanup removed normal Cargo targets and pruned coverage intermediates; a fresh compile is expected. Do not restore 27 GiB of build cache merely to resume source work.
