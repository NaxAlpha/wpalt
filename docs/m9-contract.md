# M9 — Distributed full-platform release

Active 2026-10-05 after authorized M8 merge. This contract implements the existing M9 roadmap; it does not narrow the final product or certify the existing system for multi-node use.

## End-user outcome

An owner deploys the cumulative CMS on a local SQLite site, single PostgreSQL server or documented multi-server PostgreSQL installation, with shared durable media, coordinated writes/workers, observable health and supported upgrade/recovery procedures. Deployment requires no vendor account. Final adoption/support claims require capability reconciliation and user review.

## Working sequence

1. Inventory every process-local assumption and every parity gap. Establish deployment identity/configuration, current-format exports and upgrade preflight. Reject unsupported configurations before serving. Keep the current single-process guards until their replacements are verified.
2. Implement cluster-safe domain writes and fresh authority checks, shared sessions/challenges/abuse admission, bounded cross-node cache invalidation and safe media ownership. The coordination authority must fence actual writes; holding a separate pooled advisory connection alone is insufficient when that connection can fail while another connection keeps writing.
3. Coordinate scheduled work, durable ownership, retries and externally visible side effects. Exercise crash/cancellation/lease loss and recovery. Distinguish database atomicity from at-least-once network execution; require receiver/provider idempotency where available and document ambiguous outcomes.
4. Deliver reproducible deployment, readiness, maintenance/upgrade and recovery journeys. Use explicit maintenance windows for incompatible versions rather than indefinite mixed-version shims. Establish the version/support matrix and compatibility removal lifecycle.
5. Reconcile all core/plugin capabilities and F124–F130. Implement agreed missing local capabilities or document options and realign material scope gaps before final completion. Selected M8 mapping must not be advertised as full Elementor or universal plugin parity.
6. Verify and optimize the cumulative system, provide an understandable acceptance review and one M9 delivery PR. Each preceding step first works correctly, then receives targeted verification/security/performance/UX optimization.

## Verifiable completion gates

- Two independently started application processes against real PostgreSQL and shared media; load-balanced user journeys without relying on sticky sessions for correctness. Repeat SQLite and one-process PostgreSQL journeys.
- Coordinated concurrent publication/version conflict, private-content/access revocation, form consent/actions, stock checkout/payment receipt and booking capacity preserve their invariants across nodes. Worker/process termination does not invent or lose committed domain authority.
- Cross-node invalidation prevents serving withdrawn/restricted content from an old cache. Database/coordination loss fails closed for sensitive writes. Authentication challenges and abuse budgets cannot be multiplied simply by changing nodes.
- Recover database/media/configuration coherently into a fresh installation; execute data-bearing supported upgrade, interruption/retry, incompatible-version preflight and documented restore-based rollback. No default automatic destructive upgrade on replica startup.
- Independent external monitor/fleet boundaries do not confuse origin-local health with an alert while the origin is down. Configuration exports redact secrets by default; secret transfer is explicit and private.
- Resource workloads: baseline and repeated two-node measurements with 1,000 posts/20 compositions, existing business fixtures and concurrency 1/10; bounded stress at concurrency 32; three runs for affected contention/worker recovery scenarios. Record p50/p95/p99, failures, query plans/counts, process RSS and persistent bytes. Numeric latency budgets are set from measured baseline before optimization, not invented as universal guarantees. Zero invariant violations and no unbounded queues/resource growth are mandatory.
- Current primary-source guidance, compiler floor, strict lint, security/adversarial review, real-engine/CLI/browser/cluster acceptance and independently verified exact-source clean artifact. Preserve UI measurements and inspect meaningful changed operator workflows.
- Publish support/version baseline only after the user reviews final high-level journeys and the complete capability reconciliation. Licensing remains an explicit unresolved adoption decision; do not invent a license on the owner's behalf.

## Preparation findings

`App::mutation` and cached-read guards currently serialize only within one process; cache generation is an in-memory counter. Spam challenges, passkey ceremonies, login/protection limits and some worker/history/media/recovery locks are also local. The CLI data-directory lock correctly prevents local conflicting operations but is not a cross-server database fence. Some mail/campaign claims already use PostgreSQL row locks; that does not certify every surrounding operation. These dependencies must be traced and verified before multi-node enablement.

Primary sources checked 2026-10-05: [PostgreSQL locking](https://www.postgresql.org/docs/current/explicit-locking.html), [transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html). Row/transaction ownership and connection-loss behavior inform fencing; PostgreSQL documentation is not evidence that application coordination is implemented.
