# D02 — Reviewed publication verification

Date: 2026-10-07 (Tokyo). Application source `8d7f793265e2980d75974ef113f0f6dc3c996ddc`; later delivery documentation/native-browser assertions do not change application behavior. [PR #18](https://github.com/NaxAlpha/wpalt/pull/18) records exact final-head CI, ready and merge status. Local defined-scope verification is complete; ready status additionally requires all applicable final-head GitHub gates and independent exact-source clean-artifact verification. Owner authorizes merge when ready, followed by actual-main build/release verification.

## End-user system and authority

Model owners opt into assigned review. Writers atomically save current content and request another active editor/admin's review. Reviewers inspect the saved working copy, approve or request changes privately; requesters can withdraw. The indexed local queue and feedback controls share existing authoring/design primitives and need no provider account.

Approval binds canonical document, title/slug, typed fields, classification identities, locale/group and discovery values under the relevant model/common policy. Legacy imported Markdown/block projections are excluded because rich editing normalizes those duplicate representations without changing the canonical document. Current content/workflow versions and participants are rechecked under the shared mutation boundary. Self/unassigned approval, forged altered publication, stale/parallel decisions, policy changes and revoked participants cannot reuse approval. Edits/restored revisions invalidate it. Reviewed scheduled work freezes exact material; edits cancel pending promotion, and worker rechecks cancel unsafe promotion while preserving earlier live snapshots.

Private notes are plain text, bounded to 2,000 bytes, escaped in HTML and absent from public projection. Only the subject's authored decisions enter their account export; other reviewers' feedback is excluded. History is limited to twenty decisions per item. Public reads do not query workflow state.

## Reproducible verification

Use the repository's locked frontend build and Rust toolchain. `TEST_DATABASE_URL` selects an isolated local PostgreSQL fixture; `WPALT_REQUIRE_POSTGRES=1` refuses false coverage. Tests create and remove separate databases/schemas. No external account/payment/model-provider credentials are required for D02.

```sh
cargo fmt --all --check
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
TEST_DATABASE_URL=OWNER_LOCAL_FIXTURE WPALT_REQUIRE_POSTGRES=1 cargo test --locked
python3 -m unittest discover -s tests/pipeline
python3 scripts/check_protocols.py
npm --prefix frontend run format:check
npm --prefix frontend run build
cargo build --release --locked
python3 scripts/cli_acceptance.py --binary target/release/wpalt
WPALT_BINARY="$PWD/target/release/wpalt" PLAYWRIGHT_MODULE="$PWD/frontend/node_modules/playwright" node scripts/browser_acceptance.cjs
```

Local results at current application source:

- **96 Rust cases pass:** one library, 77 connected acceptance, 14 business, three document, one Elementor. Real SQLite/PostgreSQL paths run. `work/d02-final-rust.log` preserves the complete run. Four D02 acceptance clusters cover exact-material publication/recovery/export isolation, competing decisions/schedule revocation, policy/revision/scheduled edits/self-review rollback, populated pagination/history/plans. Existing publication, commerce, learning, privacy, migration, media and operational guarantees remain cumulative coverage. No new line-coverage percentage is claimed.
- **11 pipeline cases pass:** `work/d02-final-pipeline.log`. Release names printed there belong to isolated mocked publisher tests, not actual GitHub publication.
- Strict Clippy, formatting, locked frontend build/format, JS/Python syntax, whitespace and 63-record guidance freshness pass.
- Complete native CLI lifecycle passes on both debug and optimized executables (`work/d02-cli.log`, `work/d02-release-cli.log`): install/login, scheduler/restart, theme/commerce/import operations, private recovery, selection/held clone and redacted diagnostics.
- Real retained schema-16 executable creates independent SQLite/PostgreSQL source sites. Current debug/optimized executables refuse ordinary old-schema startup, validate exact maintenance plans, refuse stale/existing outputs, retain authenticated encrypted original v12 bytes, migrate transactionally to 17 and restore with the old executable into fresh targets (`work/d02-actual16-maintenance.log`, `work/d02-release-actual16-maintenance.log`). GitHub separately verifies the published native schema-14 source executable; this gate must pass before readiness.
- Full single-node Chrome and two-app-process/one-worker PostgreSQL browser suites pass (`work/d02-cumulative-browser.log`, `work/d02-two-node-browser.log`). They include eleven admin routes at three widths, gallery/visual/keyboard/contrast checks, fifteen populated business screens, authoring/autosave, protected access, commerce, recovery, integrations and editorial review. Local processes do not certify physical multi-host deployment.
- Optimized editorial browser journey additionally passes JavaScript-disabled native writing/request/publication (`work/d02-release-editorial-browser.log`). Chrome **154.0.8037.98**; queue geometry at 320/768/1440, reviewer editor at 320/1440; two focused axe scans report zero violations/incomplete findings. No script errors or external requests. These focused scans do not certify whole-system accessibility. Actual narrow/wide captures were inspected: cardless hierarchy, legible wrapped queue and clear decision controls; existing long authoring canvas requires scrolling. No numerical aesthetics claim.

## Measured work and footprint

[Populated queue observations](d02-queue-observations.json): 1,001 work records, 101 matching assignments exactly once in three cursor pages, 20 retained decisions after repeated reassignment. SQLite uses `editorial_assignment` and the post primary-key index. PostgreSQL uses a backward assignment index scan plus bounded post lookup, 41 rows/167 shared buffer hits, observed execution 0.108ms. Thirty single-client debug in-process router samples: p50/p95 **5.05/5.85ms SQLite**, **5.74/6.37ms PostgreSQL**. These fixture observations include private session/router/render work but no network/browser or production workload. No timing assertions force machine-specific budgets/plans.

Local optimized macOS ARM executable: **28,682,352 bytes (27.35 MiB)**, SHA-256 `271c9dec73f672c91ef8c8cee900cde84ce7ea873ca88b94ff86704567a7a4fd`; build 5m46s. Preserved at ignored `work/d02-runtime/wpalt`; source `8d7f793`. This is not the Linux clean artifact and not a measured resident-memory/runtime-site disk claim.

## Recovery and supported limits

Native schema 17 / portable v13 adds validated workflow/history tables. Ordinary runtime/restore accepts current schema/format only. Stopped 14–16 maintenance retains the original v12 point for retained-old-executable fresh rollback. `migrate-recovery-v12` explicitly produces a **new** validated current archive, retaining encryption with an independent key; it does not overwrite or restore legacy data silently. Explicit old-format inspection discloses its conversion boundary. Selection preserves workflow children; destination clones lose publication approval. Compatibility retirement is recorded separately.

Current editor/admin roles retain broad content access. D02 is not restricted-contributor/custom-role authorization, configurable multi-stage/multi-approver workflow or full premium-plugin parity. Reviewer choices initially cap at 128 accounts; queue pages cap at forty items, scheduled cycles at fifty candidates, note/history work is bounded. Optional email delivery is not configured or claimed: assignments are actionable locally. Docker/PHP are unavailable here; free PublishPress Statuses 1.3.6 source was inspected unexecuted, with no code copied; premium reference is documentation-only. Further useful breadth remains visible in the backlog.

## Delivery provenance

D01 PR #17 merged as `051d319c72510c5a61f8bd1b927a7e0e4d525df6`. Its actual-main clean artifact was independently verified against exact source/assets/lockfile/guides: archive SHA-256 `4678321c7ce3d41f51bf3b25637f2b3802609f1a21c79916bea347e65c8e0521`, Linux executable SHA-256 `10f7b8b4cc9887e0b5570d5fda0132aaff6eb0bc5a1d654ec05134344b78e2b0`, 34,648,200 bytes. Main run 37553889290 determines release publication separately. D02 exact final-head checks/artifact and merge receipt are attached to PR #18 to avoid self-referential evidence commits. [Development ledger](d02-progress.md) retains discovered failures and corrections.

## Final-head delivery and automatic merge

All seven required jobs passed on [run 37554698723](https://github.com/NaxAlpha/wpalt/actions/runs/37554698723), head `29f9384ec39a5b594931bfb181a9007846fe46fd`: compiler floor, dependency audit, application, clean build, native PITR, frontend and local processes. Optional reference workloads were skipped according to their workflow conditions; they are not claimed as rerun results.

The independently downloaded clean archive verifies GitHub's PR merge source `3030e67201a57bf924d1dea807c44cac0392b32f`, locked dependencies/frontend assets and exact packaged operating guides. Archive SHA-256 `1856305f9bade54816a0bbd95399916ee4e28f0a6526fb11496e543d2771a1d9`; executable SHA-256 `b5e66956b358306fb2c731e86e1af9ea482f355586962be65a3baa162f63188f`, 34,964,808 bytes (Linux x86-64).

[PR #18](https://github.com/NaxAlpha/wpalt/pull/18) merged at 2026-10-07T01:08:05Z as `eaddaca4b768ea4a83ae55e003bb76029f7d265c`, under the owner's ready-PR automatic-merge authorization. [Actual-main run 37555571713](https://github.com/NaxAlpha/wpalt/actions/runs/37555571713) is a separate release gate; publication and independent main-archive verification remain pending until that run completes.

The actual-main run completed successfully, including all seven required jobs and release publication. [nightly-202610070108-eaddaca4b768](https://github.com/NaxAlpha/wpalt/releases/tag/nightly-202610070108-eaddaca4b768) is a published development prerelease. Both the main clean-build artifact and separately downloaded public release archive were independently verified against `eaddaca4b768ea4a83ae55e003bb76029f7d265c`. Main archive SHA-256 `ec8f28432d612b0da4db5d73950133a920e0fc2d7a1245b464a7c26c82d77446`; Linux executable SHA-256 and size match the independently verified PR executable above. Publication is confirmed, rather than inferred from a successful PR build.
