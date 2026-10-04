# M6 verification record

Defined scope and delivery gates for [PR #12](https://github.com/NaxAlpha/wpalt/pull/12), based on merged main `55cc1d8538e07bf01e4e2827b26379f58da00608`. Read [the contract](../m6-contract.md), [operator guide](../commerce-reservations.md) and per-capability [parity mapping](../feature-parity.json). Ready status requires the **final source** to pass all five PR jobs and independent clean-archive verification. Human review/merge and actual provider-account certification remain separate.

## Readable connected journeys

`tests/support/commerce_journeys.rs` adds twelve substantial journeys to the cumulative suite. Each runs on SQLite and on real PostgreSQL when `TEST_DATABASE_URL` is set; CI requires PostgreSQL 17 rather than silently skipping it.

1. Snapshotted prices/tax/shipping, stale quotes, actual payments, customer privacy, partial/full refunds, stock and whole financial graph recovery.
2. Simultaneous last-unit and last-slot checkout, abandoned hold reuse, assigned-staff overlap, confirmed reminders, refunded capacity.
3. Paid recurring access, integer prorated upgrades, settlement and revocation restricted to the refunded purchase's grants.
4. Renewal/dunning, accepted downgrade prices despite later catalog changes, paid periods, cancellation and recovery.
5. Current member prices, coupons, real settled referral commissions/manual payouts, no implied marketing consent, atomic free orders.
6. Malicious financial/stock/access/slot archives rejected before recovery writes.
7. Real local HTTPS payment protocol fixture: default TLS trust denial/custom CA, raw signatures/freshness/API version, changed replay payloads, canonical amount mismatches, pending/completed refunds; initial invoice before Checkout, later renewals and delayed old callbacks; cancellation; full subscription refund with failed external billing-stop transmission, persisted recovery/retry; money arriving after cancellation retained for refund without renewed access. The fixture enforces request authorization/version/idempotency and is explicitly not Stripe.
8. Populated 1,000-product/variant and consistent 1,000-order/line graph, twenty-line cart, thirty warm quote/private-request observations, all 1,000 private orders reached once through bounded pages, actual SQL plans.
9. Protected digital download/revocation, quota rollback, late real money without stock allocation, module-off and fresh account authority, recovery.
10. M4 one-time promotion reward redemption, financial retention after consented privacy erasure and no reusable coupon after recovery.
11. Schema-8-to-9 site/account/publication preservation, reserved-route rejection with transactional rollback, retry and stored-currency protection.
12. Long calendar cursor access, resource reassignment overlap, stale edits, paused resources and disabled assigned-staff rejection.

The full local SQLite suite includes 39 connected acceptance journeys, 14 business/rule journeys, existing protocol/credential checks and warnings-denied Clippy. PostgreSQL and compiler-floor verification are final CI gates. No timing threshold is used as a substitute for correctness.

## Native experience and shared frontend contract

`scripts/commerce_acceptance.cjs` participates in the existing cumulative real-browser suite. Merchant and shopper complete physical purchase, authoritative cart review, actual recorded payment, fulfillment/full refund; resource/group-slot creation, paid reservation/cancellation request; and paid subscription/future-billing cancellation. Fifteen meaningful screens produce **90 geometry/text-spacing measurements at 320/768/1440** and **15 accessibility scans**, with script/remote-request/geometry failures rejected. Local focused verification passed before the final cumulative optimized-binary run; exact-source final results accompany the PR review packet and CI artifacts. Screenshots require direct visual review, not an automated aesthetic score.

The visual review checks calm cardless hierarchy, shared typography/colors/control geometry, readable merchant/customer financial states and narrow wrapping. Browser measurements do not certify whole-application accessibility or every theme. Native resource/plan/discount pickers are bounded as documented; universal plugin-interface parity is not claimed.

`scripts/cli_acceptance.py` verifies real install/configuration precedence/process lock/login/scheduler restart, previous features, commerce JSON imports with invalid-price rejection, demo/maintenance without simulated payment, and fresh whole-site restore retaining prices/stock/calendar. Debug logs are checked for useful events/timing and absence of passwords, CSRF values and session cookies.

## Queries, runtime and delivery

`work/m6-volume.json` retains both engines' actual plans/latency observations. Customer history, published catalog, cart and booking lookup indices are exercised; quote pricing uses a bounded joined projection and entitlement lookup, catalog uses indexed product/variant projections, and row quotas use derived transactionally enforced counters. Page/line/capacity/provider-body limits are documented. These checks do not claim every conceivable configuration or distributed workload is optimal; M9 owns cross-process coordination.

The reproducible `scripts/benchmark.py --composed --discovery --commerce` probe records optimized binary hash/bytes, 1,000-post SQLite blog plus four-product/slot fixture, fourteen warm localhost request scenarios, RSS, persistent site bytes and log bytes. Normal and debug logging observations are recorded separately. They include Python-client overhead and exclude browser rendering; no current matched WordPress comparison or production capacity claim is made.

Final delivery requires fmt/Clippy/full SQLite+PostgreSQL suite, 32 fresh feature-guidance records, release safety tests, dependency audits, Rust 1.85 floor, an independent empty-Cargo/target Linux build, regenerated locked frontend, cumulative CLI/browser against the exact clean archive, and archive source/locks/assets/docs/binary/hash verification. PR checks publish evidence artifacts; only a later human-authorized merge publishes the dated automatic release. Exact final source/run/archive identities are retained in the local M6 review packet and PR description.

## Limits and migration

One owning process per site, integer money and one immutable USD/EUR/GBP/JPY currency. Merchant-set tax/flat shipping are local rules, not universal compliance datasets. Authenticated checkout, fixed single-item hosted recurring collection, defined offline proration and explicit UTC slots. Guest checkout, hosted plan changes, carrier labels/rates, external calendar synchronization and certified tax filings are not claimed. Stripe API `2026-09-30.endive` is pinned from current primary guidance; no live/sandbox-account credential evidence exists. Operator-visible pending work is retained on uncertain provider transport.

Schema 8 upgrades to 9 after resolving newly reserved `shop`/`commerce` content routes with the older runtime. Current archive v8 validates/replays the full graph and pending service/provider work; legacy M5 archives restore in the matching old runtime before upgrade/re-export. Unpublished M6 preview data may reset; no compatibility parser is retained. Financial/customer/provider archives require secure independent retention. Scheduled/encrypted destinations remain M7; multi-server operation remains M9.

## Preceding release verified

PR #11 merged and [actual-main run 37167056070](https://github.com/NaxAlpha/wpalt/actions/runs/37167056070) passed all six jobs, publishing [nightly-202610040107-55cc1d8538e0](https://github.com/NaxAlpha/wpalt/releases/tag/nightly-202610040107-55cc1d8538e0). Its downloaded public archive/SHA256SUMS matched the gated build byte-for-byte; source, locks/assets/guides/executable were independently checked before M6 delivery.
