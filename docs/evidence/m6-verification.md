# M6 verification — work in progress

2026-10-04. Branch `milestone/m6-commerce-reservations`, based on merged main `55cc1d8538e07bf01e4e2827b26379f58da00608`. No M6 completion claim yet.

The first automatic public release from delivery PR #11 was independently verified: [nightly-202610040107-55cc1d8538e0](https://github.com/NaxAlpha/wpalt/releases/tag/nightly-202610040107-55cc1d8538e0), run 37167056070, all six jobs passed. The public archive/SHA256SUMS match the gated build byte-for-byte and its locks/assets/guides/source/executable checks pass.

Local cumulative Rust tests pass on SQLite after the initial M6 integration. Seven connected commerce journeys cover snapshotted orders/refunds/recovery, concurrent last unit/slot allocation, paid recurring access/refund ownership, renewal/dunning/downgrade price preservation, member pricing/discount/referral/payout/free settlement, tampered financial recovery rejection and a real HTTPS provider fixture with TLS trust/raw signatures/replays/canonical amount mismatches/pending versus completed refunds. This is deterministic transport-adapter evidence, not Stripe sandbox-account verification.

Remaining delivery gates: extended commerce boundaries/provider recurring scenarios, real PostgreSQL run, populated query/footprint observations, full current-source native/browser/CLI checks, screenshot review, guidance/parity reconciliation and independently verified clean delivery archive. Update this record with exact source/run and final results before marking the delivery PR ready.
