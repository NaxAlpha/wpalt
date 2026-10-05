# M9 coordination inventory

Preparation inventory, 2026-10-05. This records dependencies, not distributed certification. Every replacement needs a connected cross-process journey, not a mock-lock test.

| Boundary | Current authority | Required distributed invariant / failure evidence |
|---|---|---|
| Domain mutations | `App::mutations`, per-process mutex; native domain transactions | Admission and actual domain write share a database fencing/transaction authority. Terminate the owner/connection while another node waits; stale owner cannot commit stock, booking, access or publication decisions. |
| Cached public responses | In-memory even/odd generation and mutex; scoped allowlist | Withdraw/restrict/edit on A is visible on B before its next eligible cache hit. Version read and authorized rendered state must have a coherent boundary; never use TTL alone to protect withdrawal. |
| Sessions and delegated grants | Database records with fresh password/role/expiry checks; mutation wait local | Cross-node revocation and role/password change invalidate the next protected action, including already admitted waiting writers. |
| Passkeys | Durable credentials; private server-memory single-use ceremonies | Registration/authentication can finish on another node without replay; consume challenge atomically and preserve live account/session/password/credential-version checks. Persisting WebAuthn internal state requires reviewing library serialization/security contract. |
| Login/protection limits | In-memory limits per process | Admission budget is site/account/client bounded across nodes and restart; expiry/cleanup bounded. Database failure must not silently disable sensitive limits. |
| Spam tokens/replay | Per-process random secret/used-token memory | Form issuance/validation works across nodes and replay is consumed once; keys and replay capacity have explicit durable ownership/rotation. |
| Mail and campaigns | Some PostgreSQL SKIP LOCKED claims; process-local scheduling/history | Claim expiry/worker death preserves selected delivery guarantees. No exactly-once SMTP promise. Provider ambiguity and deduplication are explicit. |
| Scheduling and recovery | Per-process job/recovery locks, filesystem cycle journal | Multiple nodes cannot collide on maintenance, publication, backup files or destructive cleanup. External worker mode and history are owner-readable, durable and bounded. |
| Media/assets | Local files, private path protections, work semaphores | All nodes see committed media; incomplete uploads/derivatives are not published. Deletion/recovery and reader interaction remain safe with supported shared-store semantics. |
| Clone/maintenance hold | Startup-loaded atomic plus persisted hold | No node continues side effects/public access after a cluster-wide hold; recovery/maintenance is fenced and owner-controlled. |
| Audit/operational files | Private local append/replacement under local locks | Records remain redacted and node-attributed; rotation/recovery avoids cross-node file clobber. Logs are not a replay queue. |
| Schema startup | `App::open` automatically migrates | Replica startup does not race schema changes. Supported version preflight and controlled maintenance upgrades protect meaningful data; incompatible binaries fail before serving. |
| Offline CLI / host lock | Exclusive `.wpalt.lock` in one data directory | Cross-host administrative changes require cluster maintenance/authority; local host lock remains useful but is not sufficient. |

## Capability reconciliation preparation

The parity matrix currently has 12 `planned` entries and one explicitly incomplete broader Elementor compatibility group. Status counts are not a coverage score.

- F009 translation duplication/synchronization: translation groups exist; automated duplication/synchronization still requires a precise workflow and evidence.
- F010 multilingual slugs/metadata: existing real-engine `multilingual_publication_keeps_drafts_private_and_variants_reciprocal` checks locale slugs, canonical/hreflang and draft metadata isolation. Matrix status appears stale; reconcile exact scope after reviewing its full journey and current release result.
- F011 local-model automatic translation: M8 local content generation is not translation evidence. Provide explicit model/task/review behavior and actual language reference, without claiming correctness from a model response alone.
- F031 internal-link/orphan analysis and F034 readability/keyword heuristics: distinguish actionable local diagnostics from search ranking guarantees; inspect existing schema/editor support before implementing.
- F124 module ownership registry and F125 unified configuration/schema transfer: preserve integrated ownership, known schemas, exact review and private/redacted secret transfer.
- F126/F127 worker history/retry/system execution: existing local journal and feature-specific claims are partial; independent coordinated worker/restart journeys remain required.
- F128 safe-mode/rollback: held clones/local recovery exist; cluster-wide maintenance and supported upgrades are separate obligations.
- F129 developer hooks/snippets: external integration API exists; assess owner-authorized hook outcomes and avoid inventing an unreviewed in-process executable sandbox.
- F130 owner-hosted fleet: independent controller must authenticate scoped nodes, isolate failures and expose meaningful state; no vendor account dependency.
- F024 full Elementor compatibility remains incomplete. The M8 selected projection/loss contract cannot certify universal widget/style/condition/runtime or pixel behavior. Any final scope realignment requires an explicit decision and user alignment.

Licensing and public adoption/support policy remain unresolved owner decisions; no license or support guarantee is silently assigned by this audit.
