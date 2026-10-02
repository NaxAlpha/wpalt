# M4 — Business and audience platform

Active, 2026-10-02. M3.5 PR #7 is merged; its actual-main build has been independently downloaded and hash-verified; see the M3.5 delivery evidence. This contract implements the approved M4 roadmap. It is not a completed-feature or full plugin-parity claim.

## End-user system

An owner publishes a landing page with an integrated form, receives a validated submission and protected attachments, manages entries and contacts, obtains explicit audience consent, composes and schedules communications, inspects delivery and failure recovery, and measures consented engagement/conversions using local data. The editor, field grammar, permissions, administration and backups remain shared with publishing.

## Functional steps and reviewable outputs

1. **Forms and workflows:** usable visual field/step designer, draft/live publication and previews; shared typed definitions; server-evaluated conditions, calculations and scores; repeatable inputs and multi-step navigation; protected uploads; acknowledgment/signature evidence; surveys/polls; partial save/resume and explicit device draft/offline retry. Published form blocks integrate into the structured content editor and themes. Entries support bounded search, exports, notes and assignments. Registration requests use ownership verification/approval and a non-administrative account; post-creation actions create moderated drafts only.
2. **Audience:** contact detail, list membership, typed attributes and segments; explicit purpose/policy-version consent and confirmation; withdrawal, suppression, export and deletion. Submission-to-contact/list transitions are transactional and idempotent. No visitor may gain administrative rights or opt another address into campaigns without confirmation.
3. **Communications:** shared structured message composition, transactional notification rules and conditional routing; manual/scheduled/triggered campaigns; durable bounded recipient expansion and delivery queue; local file outbox and configured SMTP/host-operated relay, logs, retry/resend, fallback and visible uncertain outcomes. Provider outage must preserve records. Queue ownership and lease recovery are verified across real database connections. No exactly-once internet delivery or inbox reputation guarantee.
4. **Engagement:** explicit consent controls and first-party pageviews/events/conversions, dimensions, funnels and local reports; stable A/B allocation; scheduled/device/referrer/frequency targeting and accessible popups; locally allocated offers/wheels; bounded heatmaps and masked wireframe/session interaction playback. Never capture field values, credentials or arbitrary DOM text. Respect withdrawal and configured browser privacy signals. Disabled business/analytics features avoid unrelated assets, event capture and background work.

Each step first works as an end-to-end slice, then receives targeted correctness/security/concurrency/UX and query/resource verification. Integrate and optimize the whole system before its delivery PR is marked ready.

## Traceability and dependencies

- F021: entry/contact notes and assignments.
- F037–F047: form design, conditions/routing, steps/repeaters, stored entries/exports, uploads, calculations, signatures, surveys, partial/resume, offline capture and moderated registration/content actions. Record the actual supported field/file/input limits and evidence for each; do not infer all premium-plugin behavior from a group name.
- F084–F091: consented events, reports/funnels, bounded masked interaction playback, experiments/targeting/popups and local offer allocation. Store-specific purchase events/redemption integrate with M6's commerce system; retain that dependency explicitly.
- F094–F098: configured SMTP/routing, logs/fallback/retry, templates/notifications, consent/lists/segments and campaign/automation workflows.
- F099: integration with an owner-operated outbound SMTP/MTA on the host or chosen infrastructure. A Rust CMS queue is not a replacement for internet-wide mail infrastructure; the local outbox works without it.
- F048/F100/F101: optional operator-configured outbound handoff/transport boundaries. Avoid arbitrary visitor-chosen destinations and credential leakage; test owned loopback adapters, never send real third-party mail during development.
- F049/F050/F092/F093/F102: external payment/CAPTCHA/datasets/identity/reputation boundaries from the existing catalog. They do not become mandatory local prerequisites. Gateway checkout belongs with M6; global proprietary datasets/identity/inbox reputation are not recreated in M4.

Record defined coverage and remaining behaviors in the parity matrix. Routine dependency sequencing preserves requirements; a material scope reduction requires a documented user decision.

## Invariants and operations

SQLite and PostgreSQL are first-class. Use parameterized bounded queries, keyset pagination, small projections, useful indexes and actual query plans. Submission retry cannot duplicate entries, contacts, campaign recipients or workflow actions. Never execute user expressions/scripts. Hidden values are discarded and derived values recomputed server-side. Publication/version changes, stale submissions and permission changes have explicit outcomes.

Consent is checked before recipient expansion and immediately before delivery. A withdrawn recipient cannot remain queued for marketing. Network acceptance and database commit cannot be made atomic: ambiguous SMTP results must be visible and handled conservatively, with documented manual recovery rather than a false exactly-once claim. Attachment/recovery capabilities are random, hashed, scoped and expiring; private data never enters debug payloads or public caches.

Use typed CLI/configuration and clear administrative controls, with credential redaction and explicit transport/module defaults. Extend current-format backups to all meaningful M4 state and private files, test fresh restore, and document a one-off schema/document upgrade with rollback by independent backup. Do not retain parallel legacy request/rendering paths.

## Delivery evidence

Readable clusters establish visitor consent → conditional form → duplicate-safe entry/contact → confirmation/list/segment → queued communication → outage/retry/withdrawal → local conversion report. Include malicious/unauthorized submissions, uploads, calculations, signatures, partial/offline recovery, stale publication, concurrent claims/last-offer allocation, opt-out/export/deletion and backup recovery on both real engines. Exercise the real browser at measured narrow/desktop widths, safe keyboard interactions and the shared editor. Use current primary guidance and feature-maintenance records.

Measure submission latency/query count, report plans and event growth, recipient expansion/queue throughput, disabled-feature costs, binary/RSS/disk and representative UI interactions. Publish conditions and observations; avoid noisy performance assertions and exhaustive shallow test permutations. Create an M4 delivery PR with current-head CI and verified exact-source clean artifact; leave review/merge separate.
