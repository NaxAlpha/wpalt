# D01 verification ledger

Date: 2026-10-06 (Tokyo). Delivery [PR #17](https://github.com/NaxAlpha/wpalt/pull/17) remains a draft until current-source gates pass.

## Classified-directory implementation baseline

Application source `f78eb81ee3d5c5e7489f2d13d5a0361808dcfb3b`: all 92 Rust cases passed locally with real SQLite and PostgreSQL 17.11; acceptance contains 73 cases. Strict Clippy, formatting and all 11 release-pipeline cases passed. The optimized macOS executable is 28,433,536 bytes (27.12 MiB). These are source-specific observations, not new instrumented coverage or a footprint measurement for a populated deployment.

The final committed Studio disclosures, typed authoring, parent-category archive and integrated browser journeys passed macOS Chrome using a fixed executable. Eleven administrative surfaces at three widths, the populated fifteen-screen business cluster, control geometry/contrast/keyboard states and unchanged visual baselines passed. No external requests or script errors were observed. The directory browser supplies 320/1440-pixel screenshots. This is sampled automated accessibility and visual review, not comprehensive conformance or an aesthetic score.

Real-engine directory recovery includes typed ordered relationships, private/unpublished target suppression, ancestor filters, draft/live classification isolation, rejection of incompatible definitions and stale edits, and unpublication. A 1,001-record fixture returned all 86 eligible archive records exactly once over five pages; both engines use indexed term membership. Initial timings ran alongside compilation/other tests and are not a quiet production benchmark. Query-plan evidence is for the exercised membership path, not every possible query/settings combination.

Earlier checks exercised source-schema-14 projection and the actual retained schema-15 executable through stopped upgrade and fresh rollback. Final-source CLI and multi-process checks must be repeated before readiness. The generic version fence must not migrate occupied older data through init, restore, inspection or ordinary runtime commands.

## CI failure and response-write correction

[Run 37473787576](https://github.com/NaxAlpha/wpalt/actions/runs/37473787576), PR merge source `c593a0cfbb5b381ccb4ec3fc2a1e77b359f5be3e`: compiler floor, dependency audit and independent clean build passed. All 73 acceptance cases passed, but an existing private-attachment/follow-up case reported a database-operation error; the captured output does not identify its engine or database error code. The same case passed locally in isolation, and all 14 business cases passed in the full local baseline. Do not call this a proven CI-only failure or silently waive it.

Inspection found the response follow-up transaction did not own the shared mutation guard and read before acquiring its write boundary. The correction acquires the guard and makes a form-scoped insert the first transaction statement, avoiding SQLite deferred-read snapshot promotion. The existing real-engine response journey now proves that follow-up waits while a snapshot owns the guard; stale writes, private search and fresh recovery remain checked. Subsequent source-specific local/CI results are required below.

The independently downloaded clean archive matches source `c593a0cfbb5b381ccb4ec3fc2a1e77b359f5be3e`: archive SHA-256 `03b58795774e1f07dc2a06ffda99b5a961a93b637b116bc15a5d31d315d598c4`, executable SHA-256 `7768b02e35bcf7dac45a7844147618c1b139de35b7351206faa2a4fa8e87dd3e`, Linux executable 34,643,400 bytes. This verifies that artifact, not release readiness of a later fix.

## M9 main release resolved

Actual-main run 37372040437 completed successfully after retrying two cancelled jobs; there was no failed application assertion in that run. Published development prerelease `nightly-202610052048-c0f9f4086fd4` was independently downloaded and verified against full source `c0f9f4086fd45501186930ae7a97df9c84f52a33`, locked dependencies, frontend assets and every packaged guide. Archive SHA-256 `a06a603b77f7bece31149a2e55c7fd4bf143f23b2fb37ddda3e7b055fa3d9cce`; Linux executable SHA-256 `f08a7cd268e0f3ed83da808fd27dab23bd6875d39a474dfe5cbc48996868ca33`, 34,547,560 bytes. That release remains native schema 15; D01 targets 16.

## Remaining gates and limits

Current correction requires connected business regression, strict lint, frozen-source runtime/maintenance checks, single-node and two-node/worker browser journeys, and all CI gates plus current clean-artifact verification. No percentage coverage has been measured for D01. Historical M9 coverage is not current D01 coverage.

No premium executable was tested. Specialized vendor field families, computed/bidirectional relationships and searchable selectors beyond the current latest-128 picker remain tracked gaps. This package is a bounded native classified-directory delivery, not universal ACF/WordPress parity. Real multi-host deployment, account-dependent services and public license/support choices remain separate backlog work.
