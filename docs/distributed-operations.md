# Local process operations — M9 under verification

The working M9 topology uses independent processes on **one Unix host**, PostgreSQL and one shared private site directory. It is opt-in and still undergoing whole-system verification. SQLite retains its single-process mode. Do not treat a shared network mount or multiple physical servers as supported by this local coordinator.

## Configure and run

Initialize/recover the site while all servers/workers are stopped. The normal owner configuration includes the PostgreSQL URL, shared `data_dir` and one public `base_url`. For runtime add `local_processes = true`. Each node uses that same effective configuration, with a different `listen` address; `debug` can differ. Other settings must agree, including optional features, providers and resource budgets. The shared runtime identity also binds the exact executable SHA-256 and compiled consent-script manifest. Matching package/schema numbers alone do not permit mixed executable or script bytes; replacements require stopped-host review/resume. Keep configuration/credentials private.

For example, node A listens on `127.0.0.1:3001`, node B on `127.0.0.1:3002`; both have the same public origin handled by an owner reverse proxy. Start each with:

```sh
wpalt --config node-a.toml serve --external-worker
wpalt --config node-b.toml serve --external-worker
wpalt --config node-a.toml worker
```

`worker --once` performs one coordinated cycle for an owner-operated system scheduler. Embedded scheduling remains available when `serve` omits `--external-worker`; coordinated cycle timing avoids multiplying normal polls. No sticky sessions are needed for correctness. The test fixture uses a round-robin origin and never retries writes. Configure production proxies likewise: a connection failure does not establish whether an operation committed, and a blind retry can duplicate external work.

Requests and worker cycles serialize under a host-owned admission lock. This protects existing domain/permission boundaries and shared temporary security state. Adding processes does not promise write-throughput scaling. Waits are bounded to ten seconds and each process retains finite admission/request budgets; HTTP timeouts remain configured separately. Shared state is capped at 4 MiB and stored privately; temporary passkey state never goes to the client or portable backups. Do not manually remove or replace coordination/lifecycle files while processes are running.

All servers/workers hold a shared lifecycle lock. Ordinary offline CLI changes, snapshots, restoration and reconciliation require the exclusive lock and therefore all nodes stopped. Runtime checks native schema 15 before serving and does not migrate it automatically. The database binds to one canonical site-directory digest; a second directory for the same database is rejected before migration or runtime admission. Moving to another directory requires fresh-target database/media recovery, rather than creating an independent host lock for existing authority. `/health` checks readiness; request diagnostics and `x-wpalt-node` identify an opaque node UUID without disclosing hostnames or credentials. Operations displays the current local-process boundary and node identity. `coordinated_request_completed` records admission wait, state finalization and total coordinated duration under the same request identity as native handler diagnostics; native handler duration alone excludes those phases. Refusal reasons are static codes, never raw server state or tokens.

## Interrupted operations and resume

An operation entering native mutation authority records durable intent before executing. A writer crash, cancellation, internal failure or timeout after that boundary leaves the site paused. Parsing failures and incomplete request bodies before mutation do not create a persistent site pause. State completion syncs shared security/cache generation before clearing intent; all processes use the same admission boundary. This is conservative recovery, not automatic failover.

1. Stop **all** nodes/workers and any external integration workers. Inspect correlated diagnostics, native domain state, recovery points and separately operated mail/payment/identity outcomes. Do not infer delivery or settlement from a missing HTTP response.
2. Run `wpalt --config node-a.toml local-resume`. It requires stopped nodes, drained application database operations and a complete valid native recovery graph. The report hashes private state/intent, current configuration and canonical domain content; it does not display temporary server secrets.
3. Review the report and reconcile external effects. Then run:

```sh
wpalt --config node-a.toml local-resume --execute REVIEWED_PLAN --acknowledge-external-effects
```

A stale plan or missing acknowledgment fails before changing state. Resume rotates the spam key and clears temporary passkey/replay ceremonies; abuse budgets are retained. Configuration changes require this same explicit review. It does not replay, settle or undo an external operation. Keep original records/recovery packages until verified.

4. Restart processes and check readiness, committed records and affected journeys. If the native graph is invalid, use the documented fresh-target recovery path; do not discard intent to bypass validation.

Graceful server/worker shutdown drains an active worker cycle for up to 60 seconds. If draining fails, inspect the pause/side-effect boundary before restarting. A separate worker exits on unreconciled coordination failure rather than silently taking over uncertain work.

## Module admission and ownership

`wpalt --config site.toml modules` reports `wpalt-module-inventory-v1` without opening a database or creating site state. CLI/environment precedence matches other commands; configuration validation runs first and no credentials are emitted. Engagement admission includes the business parent switch. Ownership names identify responsibility rather than permission grants or dynamic code loading. They do not claim disabled code is removed from the executable.

## Verification and remaining work

[Contract](m9-contract.md), [decision](decisions/0013-local-process-coordination.md), [ledger](evidence/m9-progress.md) and [coordination inventory](evidence/m9-coordination-inventory.md) distinguish working slices from verified release scope. `scripts/local_process_acceptance.py` uses a disposable real PostgreSQL schema and two independent processes. The browser suite's `WPALT_LOCAL_PROCESSES=1` mode exercises cumulative workflows through two nodes and a separate worker. Performance, supported upgrades/configuration transfer, full capability reconciliation and final artifact review remain mandatory M9 gates.

## Private configuration transfer

`wpalt --config site.toml config-export review.json` creates a new private redacted inspection record by default. Database connection credentials, SMTP credentials, identity secrets and Stripe credentials are omitted from reports. This record is not a runnable configuration transfer.

For an owner-controlled move, explicitly use `config-export transfer.json --include-secrets`, retain the package privately, and review it with `config-import transfer.json --accept-secrets`. The preview reports validated redacted settings and an exact plan. `config-import transfer.json --accept-secrets --execute PLAN --output target.toml` writes a new private TOML file. Unsupported application/native-schema versions, invalid settings, non-private inputs, stale plans and existing output paths fail. No database, media, provider account or temporary security state is transferred, and no site is opened. Change paths/origin deliberately before reviewing a fresh plan; run `--config target.toml config` to inspect effective CLI/environment precedence. Securely remove secret transfer copies according to your storage policy after the target is verified.

## Supported maintenance upgrade

Normal `serve`/`worker` startup refuses older native schemas. For the M8 native schema 14 to M9 schema 15 transition, stop every node/worker, retain the old executable/private configuration and independent recovery key, then run `wpalt --config site.toml upgrade`. Preview opens the supported graph without migrations and binds the exact data/configuration and target executable. Create or independently retain a key using `recovery-key` before execution.

```sh
wpalt --config site.toml upgrade --execute REVIEWED_PLAN --recovery-output NEW_POINT.enc --key-file recovery.key
```

Execution verifies a new private encrypted full recovery graph before native migration. Stale plans, invalid graphs, existing recovery outputs and unsupported schemas fail before migration. Review any reported temporary-state boundary with `local-resume` before starting local-process nodes. Incompatible versions require a maintenance window; no rolling mixed-version compatibility layer is introduced. A failed migration preserves the recovery point; inspect the installed version and request a fresh plan before retrying. Roll back by restoring the pre-change point with the retained old executable into an empty database and directory, verify it, then switch the origin. Never run an older executable against upgraded data.

`scripts/maintenance_acceptance.py` exercises data-bearing SQLite/PostgreSQL upgrade, precondition refusal/retry and fresh recovery. Locally it constructs an explicitly labelled schema-equivalent M8 fixture. Linux CI additionally downloads and independently verifies the actual published M8 executable, creates the source with it, and uses it for fresh-target rollback. These evidence boundaries are distinct.

## Fresh physical PostgreSQL recovery

Portable recovery establishes fresh directory authority automatically. A physical base/WAL recovery retains the source authority row and therefore ordinary startup at a new directory is refused. After independently verifying the recovered engine and matching media and stopping/removing every source node/worker, use `upgrade --rebind-directory --source-origin ORIGINAL_ORIGIN` to review the exact graph/configuration/executable and old directory identity. The source's application connections must be absent, including idle pools; this is an explicit maintenance operation, never an automatic takeover protocol.

Execute the fresh plan with `--recovery-output NEW_POINT.enc --key-file recovery.key --acknowledge-source-stopped`. The recovered directory must be fresh and contain no copied temporary process authority. Rebind invalidates recovered sessions and integration grants, replaces the native directory binding and holds the site read-only. Review restored accounts/passkeys/identities, credentials, queues and external mail/payment outcomes before using `clone-activate --review REVIEW_TEXT`. Do not restart any source process or supervisor during this maintenance window. Unsupported schemas/engines use the portable fresh-target path. Actual physical recovery remains covered by the native PITR gate rather than inferred from a schema-equivalent fixture.
