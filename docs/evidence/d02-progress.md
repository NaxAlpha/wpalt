# D02 development ledger

Date: 2026-10-07 (Tokyo). Implementation in progress on `feature/d02-editorial-workflow`, following D01 PR #17 (merged 2026-10-07 as `051d319`). This is not a completion, production-support or full PublishPress parity claim.

Implemented functional slices: per-model review policy, private relational workflow/decisions, atomic save-and-request, versioned assigned decisions, exact canonical document/metadata/policy approval, current-role rechecks, schedule invalidation, native authoring/assignment queue, current portable graph v13 and explicit stopped native 14–16 → 17 maintenance. Public reads do not load review state. Local review requires no provider account.

## Observed verification

- The first SQLite approval journey exposed a semantic bug: historical imported Markdown/block projections change when rich editing normalizes an unchanged document. Approval now binds the canonical document, meaningful metadata and classification identities, excluding duplicate historical text projections. Forged meaningful content is still refused.
- The first two-engine run exposed anonymous QueryBuilder placeholders on PostgreSQL inside transactions. The transaction builder path now applies the existing numbered-placeholder normalization. Retesting passed the connected approval, privacy/fresh recovery, concurrent decision and schedule/revocation journeys on both engines.
- The first cumulative run passed 74 acceptance journeys but failed an old migration fixture's current-format assertion (12 rather than 13). The corrected old fixture removes the new tables before conversion; explicit M6 conversion rejects archives already carrying them.
- `work/d02-full-rust2.log`: all 94 current Rust cases passed (one library, 75 acceptance, 14 business, three document, one Elementor), including real SQLite/PostgreSQL fixtures. A further policy/revision/scheduled-edit cluster is added and still awaiting its own run.
- `work/d02-pipeline.log`: all 11 pipeline tests passed. Printed release names are isolated mocked publisher fixtures, not newly published GitHub releases.
- `work/d02-cli.log`: actual install/login, scheduler/restart, theme/commerce/import operations, private backup, selection/held clone/fresh restore and redacted-debug lifecycle passed on SQLite.
- `work/d02-actual16-maintenance.log`: retained independent schema-16 executable (`work/d02-runtime/schema16-wpalt`) created real SQLite/PostgreSQL source sites; current executable refused ordinary old-schema startup, verified exact maintenance plans and encrypted original v12 recovery, upgraded to 17, and restored original bytes with the old executable into fresh targets. Source-unavailable rollback was not simulated by opening the old database with the old executable.
- Frontend locked formatting/build, JavaScript/Python syntax, protocol freshness (63 records) and whitespace checks pass. Strict Clippy found one simplifiable predicate, corrected in the current source; rerun pending.
- Initial real-Chrome authoring setup enabled review through Studio and selected an eligible reviewer, but exact-label lookup failed on help text attached to the private review-note label. The field now has a precise accessible name. Browser/geometry/accessibility rerun pending; no complete UI verification claim.

## Remaining delivery gates

Verify added failure/restore cluster; responsive writer/reviewer browser journey and cumulative SQLite/two-node PostgreSQL browsers; populated queue cursor/plans and measured work; independent actual schema-14 maintenance and final schema-16 checks; current strict lint/full regression; clean optimized build and all applicable exact-source CI gates; independent artifact verification and review packet. Reviewer lists initially cap at 128 accounts; multi-stage policy/custom contributor roles and broader workflow parity remain explicit later scope. Test coverage percentage has not been remeasured.

Owner authorized automatic merge of ready PRs on 2026-10-07. D02 remains under verification. The focused Chrome journey now passes five geometry samples (queue 320/768/1440, reviewer editor 320/1440) and two axe scans with zero reported violations/incomplete findings; Chrome 154.0.8037.98, no script errors or remote requests. This is focused evidence, not whole-application accessibility conformance. Strict Clippy rerun passes. The additional policy/restoration/scheduled-edit journey passes both engines.
