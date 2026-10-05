# ADR 0013 — Conservative same-host process coordination

2026-10-05, implementation under verification. User directed M9 to rely on multiple local processes for now. This does not certify arbitrary multi-host mounts or promise horizontal write scaling.

## Decision

Use an opt-in `local_processes` mode on Unix with PostgreSQL and the same private site directory/configuration. Shared lifecycle locks allow servers/workers together and exclude offline CLI operations. A separate kernel file lock serializes whole HTTP admission and worker cycles. All processes load private server-side security state and cache generation under ownership; completion syncs the new state before clearing durable intent. Native mutation admission syncs durable intent before domain changes. A crash, cancellation, request timeout or internal failure after that boundary retains intent and pauses the site; incomplete request bodies and parsing failures before mutation do not let an untrusted caller permanently pause the site. An incomplete shared-state write also fails closed. Recovery is an explicit offline owner operation with full native graph validation, exact-plan binding, drained database operations and external-side-effect acknowledgment.

This is deliberately conservative: serial execution avoids pretending a pooled advisory connection fences unrelated database writes. After an interrupted process there is no automatic takeover; new operations remain blocked while an old database operation could be unresolved. Existing native transactions/constraints preserve their domain guarantees. Owner resume cannot certify an external SMTP/payment/identity outcome; it requires separate reconciliation and does not replay that work.

Keep temporary passkey state on the server, never in cookies/client proposal files/portable backups. webauthn-rs 0.5.5's explicitly enabled serialization feature permits this server-side use; its library documentation warns that client-side state permits replay. Shared file is private, bounded, single-use consumption is serialized, and expiry/account/password/session/credential-version checks remain native. Spam secret/replay state and login/protection budgets share the same boundary. Resume rotates spam authority and clears temporary ceremonies, retaining abuse budgets.

Readiness reports a pause; diagnostics carry a node UUID and request identity without raw state/tokens. Configuration identity allows different listening addresses/debug output, while other deployment/admission settings must agree. Runtime startup checks the installed schema and does not migrate it; offline operations retain explicit migration/recovery authority.

## Alternatives and limits

A new transactional executor wrapping every database operation would offer a broader database-fenced write boundary, but replacing the entire storage interface is not necessary for the user's current same-host delivery. An independent advisory-lock connection alone is insufficient if it fails while actual writes use other connections. TTL cache invalidation and process-local challenge maps are insufficient for cross-node authority. Automatic lease takeover without fencing would create a new correctness gap.

The selected mode serializes reads as well as mutations because requests can update abuse/challenge state and apparently read-only routes can have native side effects. Measure contention/tail latency before splitting read admission; never trade freshness/replay protection for a cache speed claim. It is a safe process topology, not a claim that adding processes improves throughput. More permissive multi-host execution needs a separate validated storage/fencing/failure design.

## Evidence obligations

Actual independent processes behind one round-robin origin, shared sessions and CSRF, cached publication/access invalidation, cross-node passkey signing/replay, shared abuse admission, version conflicts, stock/booking/forms, independent worker termination/drain and deterministic blocked-writer death. Verify stale/no-ack resume refusal, private files/log redaction, supported maintenance upgrades/fresh recovery, query/resource budgets and simpler SQLite/PostgreSQL regression paths. Current results belong in the M9 ledger, not this decision alone.
