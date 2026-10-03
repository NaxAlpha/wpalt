# M5 verification record

Status: final-source verification in progress on [draft PR #10](https://github.com/NaxAlpha/wpalt/pull/10). This record does not assert milestone completion before current-source CI and clean-artifact verification. [Contract](../m5-contract.md), [operating scope](../membership-learning.md) and per-group [parity mapping](../feature-parity.json) define what is delivered and its limits.

## Readable journeys

`tests/support/membership_journeys.rs` runs against real SQLite and, when required by CI, real PostgreSQL. Six connected journeys cover:

- Policy composition and exact expiry/drip boundaries; two members; direct pages/media/API/search/feed/sitemap/comment exclusion, revocation and disabled-module fail-closed behavior.
- Ordered quiz and assignment progress, concurrent replay, stale grading, draft/publication isolation, operator reset and certificate reissue, fresh recovery, malicious archive rejection before writes, removed-resource protection and deliberate release.
- Delegated last-seat capacity, unauthorized intervention, single-use concurrent gift claims, escaped/moderated discussion, group removal and profile CSRF.
- RSA-signed identity claims, issuer/audience/nonce/expiry/authorized-party/access-token binding, local subject binding and disabled accounts; browser-bound PKCE/state, wrong-browser rejection, provider outage and callback replay.
- Database row budgets rolling back a partially attempted publication, upserts counting only new records, and rebuilt counters matching a fresh restored graph.
- Maximum 100-lesson sequential learning projection with 1,000 unrelated grants; hidden locked titles, direct prerequisite denial, query plans and measured latency without fragile timing assertions.

The cumulative Rust suite includes earlier content, composition, discovery, authoring, business and adversarial tests. Loopback SMTP tests need listener permission locally; the initial restricted run failed at socket bind with `PermissionDenied`, and the same SMTP test passed with loopback permission. This is recorded separately from application failures.

## Browser and CLI

`scripts/membership_acceptance.cjs` is part of the cumulative browser runner. The connected native owner/learner journey creates a member, reusable access policy, entitlement, shared lessons, ordered course, quiz, private download and assignment; verifies a wrong attempt, prerequisite unlock, operator approval, certificate and immediate revocation, plus profile, delegated seats, moderated discussion, gifts and manual referral/commission administration. It checks 320/768/1440 geometry, 44px quiz-label hit areas, automated WCAG scans and user text-spacing overrides. Browser errors and unexpected external requests fail the journey. Screenshots and `membership-results.json` are review artifacts; automated checks are not a formal accessibility/aesthetic certification.

The first focused browser pass completed all three widths and text-spacing checks. Visual inspection of desktop member home, narrow quiz and course composition confirms the inherited calm, cardless design. The first cumulative CI checkpoint passed compilation, actual SQLite/PostgreSQL tests, audit and CLI, then exposed an upload-fixture selector ambiguous with existing M4 media; it was scoped to the upload form. The expanded browser run also caught the no-referrer gift page producing `Origin: null`; gift navigation now uses the established narrow same-origin Fetch Metadata exception, while retaining session/CSRF/token checks. Cross-site and forged-CSRF probes remain denied. The delivery gate requires a subsequent full current-source pass.

`scripts/cli_acceptance.py` adds policy/grant/revoke/protect, course import/export/publication, private file permissions and preserved course definition after fresh restore to the existing configuration/lock/restart/scheduler/log-redaction journey. Optional provider credentials are never required by the local workflow. No actual third-party provider certification is claimed.

## Performance and capacity evidence

`work/m5-volume.json` records fixture conditions and actual query plans per engine. Initial local SQLite debug observations for the 100-lesson projection were p50 approximately 1.1 ms and p95 approximately 1.6 ms; these are integration measurements, not production or browser latency claims. SQLite chose the resource primary key, covering entitlement access index and user/course/edition progress index. Progress is read once and matched through a map rather than a query or linear scan per lesson. Actual PostgreSQL plans and final-source figures must be retained with delivery evidence.

Persisted membership record quotas use transactional database insert/delete triggers, including restore, and avoid request-path full-table counting. Pending identity flows have bounded TTL/capacity; provider transport/body limits and module-disabled behavior are explicit. General single-process ownership is enforced by the existing site lock; distributed coordination remains M9. Bounded recent native list/picker sizes and CLI access to stable older IDs are documented usability limits rather than silently claimed unlimited administration.

## Delivery gates

- Current-source fmt, warnings-denied Clippy, all Rust tests, asset reproducibility and frontend formatting.
- Real SQLite/PostgreSQL, protocol freshness and reviewed dependency audit.
- Cumulative browser/CLI evidence, representative screenshot review and performance measurements.
- Clean Linux archive source/assets/docs/binary/checksum verification, tied to final PR source.
- PR marked ready with exact run/source/artifact identities; human review before merge.

## Local release runtime observation

The current macOS arm64 release executable was measured with a disposable 1,000-post SQLite blog and 100 requests per scenario. Executable: 15,929,040 bytes (15.2 MiB); RSS after load: 55,590,912 bytes (53.0 MiB); live site files including SQLite/WAL: 19,653,336 bytes (18.7 MiB), excluding logs/configuration. Full HTML home requests at concurrency 1 had p50 0.668 ms and p95 0.814 ms; concurrency 10 had p50 2.818 ms and p95 4.074 ms. These are localhost Python-client observations on macOS 26.6.2, eight logical CPUs, no response cache. They do not include browser rendering/assets or assert production capacity. `work/m5-runtime.json` records the exact measured binary hash and conditions; Linux artifact size is measured separately from the final clean build.

The full local cumulative browser pass produced 78 M5 geometry/text-spacing measurements and 39 accessibility scans across 13 screens, with zero geometry failures, reported script errors or external requests. Cumulative earlier-milestone browser journeys passed in the same disposable site. Later changes to tests/documentation must still pass final-source CI.
