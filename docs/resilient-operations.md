# Owner-operated resilience — M7 implementation guide

Implementation in progress. Only the recovery/cache slices described here currently exist; this does not declare M7 or its broad parity families complete.

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

Readable acceptance journeys cover encrypted fresh-engine recovery, separately held keys, original database unavailability, private media, wrong keys/tampering/truncation/budget rejection, retention, failed destination reporting, anonymous cache hit/bypass and protected-content invalidation. Local execution initially uses SQLite; real PostgreSQL is required by CI before verification claims include both engines. Full financial/learning recovery, browser geometry/error flows, performance/WordPress comparisons, further security/media/cleanup/PITR/incremental recovery and integrated release gates remain outstanding. See m7-contract.md; the full 27 planned families remain visible and are not marked complete.
