# Integrated business and audience workflows

Decision in preparation, 2026-10-02. M4 follows the agreed roadmap and the M3.5 structured editor. Keep form definitions, published versions, immutable accepted entries, contacts/consent, lists/segments, campaigns, durable jobs and first-party events in owner-controlled storage. Shared field semantics and bounded expressions supply form conditions/calculations; shared document rendering supplies messages and embedded workflows. No visitor-supplied executable code or destination.

Use transactional entry/action deduplication, current-consent recipient checks, bounded lease-based workers and explicit ambiguous-delivery states. SMTP cannot atomically commit with the database. A local file outbox supports disconnected evaluation; configured SMTP may point at the owner's own MTA or optional provider. Worker claims need real concurrent-engine verification, not an application-local mutex alone.

Privacy defaults are deliberate: no analytics before consent; mask all form/input values and text in interaction capture; retain bounded geometry/action data for wireframe playback, and expose opt-out/deletion/retention. This is an implementable privacy contract, not a jurisdiction-wide compliance certification or a claim to proprietary identity/reputation datasets.

The exact schema, document upgrade, queue semantics and renderer/client integration will be refined through the contract's functional steps, with measured evidence. Trace every M4 group, scope limit and M6 dependency; do not declare a broad group complete from a small demo. See m4-contract.md.

Primary guidance under review: OWASP File Upload and Input Validation cheat sheets, PostgreSQL 17 SELECT/locking documentation, RFC 8058 one-click unsubscribe, the selected mail transport's official API, and the existing WCAG 2.2/frontend contracts. Distinguish privacy product decisions from normative protocol requirements. Record reviewed versions, sources and mapped tests as the steps are implemented.
