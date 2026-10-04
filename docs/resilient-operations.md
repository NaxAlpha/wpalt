# Owner-operated resilience — M7 implementation guide

Implementation in progress. Only the recovery/cache/local-rule slices described here currently exist; this does not declare M7 or its broad parity families complete.

## Recovery keys and portable packages

`wpalt recovery-key /secure/recovery.key` creates a private 256-bit random recovery key without opening a site database. Copy that key independently offline. This is a key, not a password. Loss of all copies makes encrypted packages unrecoverable. Do not store it inside backup destinations.

`wpalt --config site.toml backup /independent/site.wpbackup --key-file /secure/recovery.key` captures the existing consistent logical database/media/private-attachment snapshot and authenticates/encrypts it using ring ChaCha20-Poly1305 with a fresh random 96-bit nonce and authenticated format metadata. The package has a versioned binary envelope. Existing plaintext manual backup remains explicit. Keys are never accepted as command-line values or embedded in packages.

With the original host unavailable, install the executable on a fresh host, configure an empty SQLite/PostgreSQL database and new data directory, bring the independently held key/package, then run `wpalt --config fresh.toml restore /independent/site.wpbackup --key-file /secure/recovery.key`. Authentication and the existing whole-graph validation precede database writes. Non-empty destinations are rejected. Existing sessions are not recovered. External credentials and host configuration remain owner-managed and must be recreated securely; the archive does not replace a deployment configuration inventory.

## Managed schedules, copies and retention

Configure `[recovery]` with `enabled=true`, private `key_file`, one to eight existing absolute `destinations`, `interval_seconds` (60 seconds to 30 days), and `retain` (1–365). Run `wpalt recovery-run` offline, or use Operations → Create encrypted recovery copies while serving. `wpalt recovery-status` and Operations display durable last attempt, last complete set and each destination's verified/failed/pending status. The server scheduler triggers due captures. Settings currently require restart; arbitrary destination paths cannot be submitted by browser users.

A destination can be a mounted independently managed NAS/disk/secondary-host filesystem. This does not implement SFTP credentials or prove physical independence. Missing directories are not recreated: a lost mount must not become an apparently successful origin-disk backup. Configure monitoring/mount policy and test actual source-host loss. A pending status after restart indicates an interrupted attempt; no partial file is considered complete. Retry waits the configured interval to bound repeated failure work, or the owner can retry explicitly.

Copies are published as private, fsynced, no-overwrite packages through temporary files and then read back/authenticated. Retention follows verified copying, preserves the current package, touches only recognized regular package filenames, and refuses to remove a damaged or wrong-key old package. Unrelated files and partials are retained for operator inspection. Directory size is bounded during enumeration. Failed destinations leave the successful copies intact and remain visibly failed; CLI returns failure if any copy is incomplete. Filesystem/hardware durability still depends on the storage's actual sync and hard-link semantics. Unsupported destinations fail rather than silently reducing guarantees.

## Bounded public cache

Configure `[cache]` with `enabled=true`, `max_bytes`, `max_entries`, and `ttl_seconds`. It stores only successful anonymous listing/search/sitemap responses with known bounded bodies, without cookies/Authorization/Range or Set-Cookie/private/no-store output. Query and language URLs are distinct. Private pages, account flows, member media, forms and commerce are excluded. Response-size budget failures are not silently truncated.

Shared application mutation guards advance the cache generation at entry and exit, including failed mutations. Cacheable rendering holds the shared read boundary to prevent an intervening committed access change between rendering and insertion. Hits receive fresh outer request/security headers; cached request IDs are removed. Entries have TTL and total-byte/entry bounds with oldest-first eviction. `x-wpalt-cache` identifies hit/miss. Restart empties cache. This is single-process application cache, not distributed invalidation or a CDN, and operator SQL outside wpalt is not an automatically observed mutation. Native database changes require restart or an explicit application invalidation boundary. Broader page/object caching, preload and performance controls remain active M7 work.

## Current evidence and remaining work

Readable acceptance journeys cover encrypted fresh-engine recovery, separately held keys, original database unavailability, private media, wrong keys/tampering/truncation/budget rejection, retention, failed destination reporting, anonymous cache hit/bypass and protected-content invalidation. The first committed recovery/cache checkpoint (1af9b71) passed real SQLite/PostgreSQL and cumulative browser/CLI CI in run 37178094283. Later local rules/media/scan additions require their own exact-source CI before inclusion in that claim. Full financial/learning recovery, browser geometry/error flows, performance/WordPress comparisons, further security/media/cleanup/PITR/incremental recovery and integrated release gates remain outstanding. See m7-contract.md; the full 27 planned families remain visible and are not marked complete.

## Local request protections

Configure `[protection]` with `enabled`, literal `denied_prefixes`, exact `denied_peers`, `requests_per_window` and `window_seconds`. Matching path prefixes use segment boundaries, not arbitrary regex code. Rate counters key the actual TCP peer, not X-Forwarded-For. Behind a proxy, this means a shared proxy counter; configure protections at that outer proxy rather than assuming forwarded user identity. Health reads bypass the rate counter but not explicit deny rules. Active peer counter storage is bounded to 4,096; exhaustion fails closed instead of evicting active counters and granting fresh budgets. Request concurrency, timeouts and body limits remain in force. This local protection is not edge DDoS absorption. Changes require restart; keep the config/CLI recovery path available if an owner deliberately blocks their administration routes.

Draft milestone delivery: https://github.com/NaxAlpha/wpalt/pull/13. Not ready for merge.

## Native image derivatives

`/media/{id}/resize/{width}` produces WebP at one of 320/640/1280/1920 pixels without enlarging originals. The same current role/entitlement checks as original media run before processing. Output is no-store, never a private-file cache bypass. Native blocking workers hold the bounded media semaphore throughout decoding/encoding; source bytes, pixels and decode allocations are bounded. PNG/JPEG/WebP are accepted; GIF derivatives are explicitly rejected to avoid silently removing animation. On-demand processing currently avoids persisted derivative recovery/cleanup complexity; AVIF, derivative reuse, video processing and richer authoring integration remain M7 work.

## Stored-file inventory scan

`wpalt integrity-scan` or Operations → Inspect stored-file integrity checks stored image/private attachment paths, regular-file metadata, byte counts and authoritative SHA-256. It does not remove or quarantine files. Work is bounded to 1,000 rows per inventory and 64 MiB total; a limited result explicitly fails the CLI success gate, rather than certifying unexamined files. Browser access requires administrator authority and CSRF. Broader inventory pagination, known-pattern malware rules and scan history are still required in M7.

Image dimensions are strict decoder limits. The image library documents allocation limits as best-effort; the configured 64 MiB decode allowance is not a guaranteed process-memory ceiling. Worker admission also bounds simultaneous decoding.

## Privileged HTTP action history

Native `/admin` and `/api/admin` writes persist a private journal intent before dispatch and response status afterward. Failure to persist intent stops dispatch. If an action committed but its outcome cannot be written, the response is preserved with `x-wpalt-audit: outcome-write-failed` and an error diagnostic; a misleading failure must not encourage blind replay. Interrupted intents explicitly lack a paired outcome. Matched route patterns, request correlation, current local actor ID, timestamp and status are recorded; bodies, query values, passwords, session/CSRF tokens and email addresses are excluded. Operations links the administrator-only recent history (200 records). Two bounded 1-MiB private files retain current/previous logs; archive externally for longer retention. This is an operational HTTP journal, not tamper-proof storage, a database transaction ledger, or coverage of offline CLI/background actions; broader shared audit integration remains M7 work. Files are not currently included in the logical recovery archive.
