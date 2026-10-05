# ADR 0011 — Selective editorial recovery and held URL-aware clones

2026-10-05. M7; one owning process. Implementation decision within the agreed recovery scope. No live financial graph merge or distributed clone activation is claimed.

## Outcome

An operator can validate a complete recovery point offline, select content UUIDs, inspect the conservative dependency closure and create a private package using the exact preview hash. Selected content retains linked content, translation groups and references from shared configuration, themes, protected-resource, learning, workflow and financial records. Revisions/comments/term assignments follow retained content. All other tables and media/private attachment originals stay intact. The complete transformed graph is validated again before any output or database write. This is editorial selection, not customer-data minimization or arbitrary table recovery.

UUID and percent-encoded/structured-string references plus local pretty paths are scanned once into bounded dependency edges; a work queue computes closure. Bounds: 1–1,000 requested UUIDs, 100,000 scanned rows, 64 MiB text and 100,000 edges. Ambiguous/external URLs whose path matches local content conservatively retain the content. Dynamic application-generated URLs or user code are not statically understood. Extra retention is disclosed. Invalid graphs or excessive budgets fail without writing a package. Execution binds source bytes and resulting validated package bytes; output is new-only and private. Archive files contain account credentials and private data even when fewer editorial posts are retained.

URL-aware cloning requires explicit old origin and target configuration origin. It rewrites a declared allowlist of presentation fields and structured JSON string values, checking origin boundaries so source.example.invalid.evil.invalid is not rewritten. It preserves credentials, immutable financial history, identifiers and queued payloads. It does not rewrite optional script files, external identity callbacks, forms, provider identities or arbitrary embedded code; review those explicitly. Ordinary snapshots already exclude sessions; clone packages additionally remove origin-bound passkeys. Accounts/passwords and local TOTP are retained for owner review.

## Durable hold and side effects

Schema 12 adds one recovery_mode row (held flag, source and target origin, operator review). Current archives are wpalt-backup-v12 and include it. A held clone is read-only through HTTP except password login/logout: writes, payment webhooks and external identity routes fail with visible 503. Background cycles are not polled, including jobs whose intent normally writes history; direct run_cycle rejects before polling its supplied future. CLI mutations are blocked while held except the dedicated stopped-host activation. Owners can inspect administration, public output and private recovery/export. Existing controls visually remain but mutations explain the hold.

Activation requires the exclusive process lock, exact configured target origin, current held state and a 40–2,000 character operator review. It is privileged/audited and preserves that review. The operator must establish whether the source is shut down; review message queues/uncertain deliveries, provider account/subscription ownership, callback URLs, user credentials and any independent integrations. Activation cannot prove external ownership transferred and does not imply exactly-once payment or mail delivery. A staging copy should stay held. Disable provider/mail credentials at the deployment/network boundary as defense in depth. All source data remain confidential.

The hold is database state, not a config flag that can be lost on restart. Capture/recovery preserve it; startup loads it once under the owning-process contract. Restore/activation update the in-process atomic state at commit. M9 must revisit distributed agreement. An offline host owner can deliberately alter the database; this is not protection against the machine owner.

## Migration and reference

Database 1–11 upgrades transactionally to 12 without deleting meaningful data. Ordinary archive restore only reads v12. For owned v11 preview archives, restore with the matching older binary into an isolated fresh target, take a verified checkpoint, upgrade that database and re-export using the current binary. The dedicated offline M6 converter adds the inactive recovery row and privacy/authentication additions. Disposable fixtures may reset explicitly. No legacy runtime archive parser is added.

Reviewed primary reference: [WordPress migration handbook](https://developer.wordpress.org/advanced-administration/upgrade/migrating/) on 2026-10-05. WordPress distinguishes generated site URLs from stored embedded URLs and warns that unsafe replacement can corrupt serialized data. wpalt operates on its validated JSON schema and declared fields; it does not edit PHP serialized values or claim WordPress import compatibility. That import remains M8.

## Evidence

Readable SQLite/PostgreSQL journeys cover dependency selection, unrelated content omission, deterministic previews, fresh-target restore, occupied-target rejection, corrupt/missing roots, origin-boundary rewrite, inherited-session rejection, held HTTP/payment writes, background future never polled, recovery of the hold, explicit activation and repeated activation rejection. Populated privacy-queue regression checks all records across pages after selecting through the index before the user join. Exact CI and final milestone evidence remain delivery gates.
