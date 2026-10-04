# M7 — Resilient owner-operated platform

Active implementation, 2026-10-04. User authorized M7 following merged M6 PR #12. This is not a completion claim.

## End-user outcome and sequence

A site owner protects, maintains, optimizes and recovers the cumulative publishing, business, learning and commerce system through integrated administration, CLI and configuration. One owning process remains the supported mode; distributed coordination is M9. First establish each working vertical slice, then verify and optimize it, then verify the integrated system. Retain all earlier acceptance journeys.

1. Recovery: scheduled consistent database/file snapshots, encrypted authenticated packages, separately held recovery keys, durable atomic publication, bounded retention, independently configured destinations and truthful partial-copy/interruption status. Recover to an empty fresh instance without the original host or a vendor account. Inspect corrupt packages before database writes. Define clone/URL and selective recovery boundaries; provide content-addressed incremental storage and native database point-in-time recovery integration where applicable.
2. Performance/media: bounded public page/object caching with publication and access invalidation; explicit private/cookie bypass; preload, cache/compression controls, compiled asset loading and scoped CSS; image dimensions/lazy loading and derivatives; bounded native media/video workers; reference-aware cleanup with preview and safe execution. Measure full-sitemap tail latency and matched WordPress capability workloads.
3. Security/privacy: local second-factor/passkey protections, request rules and rate limits, security-header controls, bounded integrity/pattern scans, privileged action audit history, spam controls, consent/script enforcement and records/data requests, owner-page cookie scanning. External intelligence, global WAF and shared spam services remain external boundaries.
4. Integrated operations: owner-visible health, failure/retry history, resource budgets, independent-copy failure, storage pressure, interrupted jobs and pre-upgrade recovery. CLI/config/admin must describe the same effective controls. No mandatory cloud account, silent successful payment/delivery replay or uncontrolled destructive cleanup.

## Requirement inventory

All planned M7 families remain required and visible: F072–F082, F103–F111, F115–F120, F122. F083, F112–F114, F121 and F123 are explicitly external/optional boundaries, with owner-selected independent copies still part of local disaster recovery. Do not mark broad families complete from a baseline implementation alone. Record supported scopes and outstanding work in the matrix and verification report; material scope changes need user realignment.

## Verification and delivery

Readable real SQLite/PostgreSQL journeys: scheduled encrypted snapshot, original host unavailable, fresh recovery of protected content and financial state; missing/wrong key, tampering/truncation, failed destination, retention and restart; upgrade/storage failure; cache invalidation/access revocation and cookie leakage; bounded media processing and cleanup reference safety; authentication/rule/audit/consent negative paths. Browser tests measure populated/error operations screens at 320/768/1440, keyboard, text-spacing and accessibility; inspect actual screenshots. Keep tests purposeful rather than multiplying internal permutations.

Record query plans, cached/uncached latency distributions, memory/disk/job costs and equivalent enabled WordPress/plugin comparisons. Verify current primary guidance, dependencies, formatting, deny-warning Clippy, locked frontend build, Rust floor, exact-source cumulative browser/CLI and clean release. Deliver a coherent M7 PR and review packet; completion and merge remain separate. No compatibility shims for preview formats. Meaningful data needs explicit migration or verified recovery, disposable fixtures explicit reset.

## Initial source review

2026-10-04: ring 0.17 AEAD documentation https://docs.rs/ring/latest/ring/aead/index.html; SQLite backup https://www.sqlite.org/backup.html; PostgreSQL backup https://www.postgresql.org/docs/current/backup.html; HTTP cache semantics https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cache-Control. AEAD authenticates ciphertext and version metadata; checksum-only snapshots are not authenticated encryption. Logical consistent application snapshots differ from engine WAL point-in-time recovery. Expand affected guidance records as each feature is implemented.
