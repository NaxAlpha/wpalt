# Operating local memberships and learning

M5 builds on the same local users, published content, media, database and recovery archive as earlier milestones. Owners work in `/admin/members` and `/admin/courses`; members enter `/members` from their account page. Local memberships, courses, grading and certificates require no provider account or network service. Payment checkout, purchase-derived access and settlement belong to M6.

## Owner and member workflow

Create an account with the Member role in User roles, or use the existing verified account-request approval workflow. Create a reusable access policy requiring an entitlement key, a group, or both. Assign that entitlement to an account with an explicit UTC start and optional expiry. Revocation and policy suspension take effect on subsequent access checks. Staff editors may read protected content; owner-only administration controls membership and grading. Custom permission-role authoring is outside this defined M5 scope.

A content or media rule protects direct requests, not just navigation. Protected publications disappear from anonymous listings, search, feeds, sitemaps and public relationship/collection projections. Public media must be explicitly assigned a rule to become protected. A file merely embedded in a protected post remains public unless its own rule protects it.

Build a course from shared published content. Its private draft includes lesson order, absolute UTC opens, delay after membership/group entry, up to twenty multiple-choice questions per lesson, bounded attempts and optional textual assignments. Add further quiz questions through the expandable question controls. Assign a lesson's downloads through its Protected downloads section: direct file requests then enforce the same entitlement, drip and preceding-lesson completion. Publish deliberately; draft course titles and settings do not alter the live catalog.

Members read unlocked lessons, take quizzes and submit work. A wrong attempt consumes an attempt without unlocking the next lesson. Retrying the same submission key does not consume another attempt. Assignments require owner approval in Members; feedback appears with the lesson. Completion allows a local certificate tied to that course edition and account. Certificates are accessible to that member and the owner while course access remains valid; they are not a public third-party credential registry.

Republishing creates a new edition and fresh progress for that edition. Historical attempts, assignments and certificates remain local records. A completed lesson requires an operator reset before new work. The gradebook reset clears current attempts/score/completion and revokes that edition's certificate; requesting changes also revokes completion/certificate. After reset and successful replacement work, a new certificate ID can be issued; the revoked old link remains invalid. Removed lesson/download resources remain protected; removal never silently publishes previously private data. Owners may use Release access rule (or `member release --kind post --resource POST_ID`) after a resource is removed from the live course. Releasing a rule does not change a file's underlying private/public visibility.

Groups support moderated text discussion. Organization managers can add/remove existing active accounts within their assigned seat limit; they cannot create owner roles or grant entitlements. Single-use gift links confer a predefined entitlement for a bounded period to one signed-in claimant. Copy links when created: the server stores only their digest. Group membership, entitlement and drip conditions compose rather than replace one another.

Referrals count visits through a fixed local redirect and provide manual commission records with unique references and integer minor-unit amounts. These are visit counts, not unique-person attribution. M5 does not claim purchases, payouts or tax handling. Profiles and discussions use escaped text; profiles remain private.

## Configuration, capacity and CLI

`membership_enabled = true` and `membership_max_records = 1000000` are top-level configuration keys. `WPALT_MEMBERSHIP_ENABLED` overrides module availability. Disabling membership removes its workflows while retaining access rules; protected resources fail closed. Capacity counts all persisted membership rows, including history. Database triggers update the counter in the same transaction as writes; an exhausted budget rolls back publication and restore. Lowering capacity below existing usage preserves data and allows reads/deletes while preventing new rows.

Owner and member course catalogs use indexed cursor pagination, forty courses per page, with access filtering before the limit. Assignment review shows the oldest forty pending submissions with their lesson and edition; grading advances the queue without burying older work beneath completed submissions. Other native lists/pickers show bounded recent sets (40 grants; 100 selections/progress rows). CLI automation can address older records by stable IDs and users by email. Searchable administration across these larger histories/pickers remains a usability refinement; do not interpret a list's absence as deletion or lost authorization. Course definitions are limited to 100 lessons and 256 KiB, with 128 distinct downloads and 1–10 attempts per lesson. Ordinary groups and organizations are bounded at 1,000 seats.

Stop the owning server before offline commands. Configuration/CLI commands use the same site data lock as backup/import. Discover flags with `wpalt member --help` and each subcommand's help:

```sh
wpalt --config site.toml member policy --title Academy --entitlement academy
wpalt --config site.toml member grant --email learner@example.com --entitlement academy
wpalt --config site.toml member revoke GRANT_ID
wpalt --config site.toml member protect --kind post --resource POST_ID --policy POLICY_ID
wpalt --config site.toml member course-export COURSE_ID course.json --draft
wpalt --config site.toml member course-import course.json --id COURSE_ID --publish
```

Course JSON uses `title`, `policy_id`, `sequential` and `lessons`; each lesson includes stable UUID `id`, `title`, `post_id`, `downloads`, `delay_seconds`, `opens_at`, `questions`, `assignment`, `pass_percent`, `max_attempts`. Questions contain `prompt`, `choices` and zero-based `correct`. CLI timestamps are Unix seconds. Private exports must be kept outside a public web root. Avoid placing personal data or real secrets in version control.

## Optional provider sign-in

The `[identity]` connector is disabled by default. Set an HTTPS site origin, exact HTTPS issuer, client ID, authorization/token/JWKS endpoints, and secret through `WPALT_IDENTITY_CLIENT_SECRET`. Register the exact callback `https://YOUR_ORIGIN/members/identity/callback` with the provider. Confidential clients use `client_secret_basic` with standard credential encoding; a blank secret supports an explicitly registered public client with PKCE. An optional `ca_cert_file` adds an owner-trusted public PEM root for private provider PKI (regular file, at most 32 KiB); certificate verification remains mandatory. Only authorization-code flow with S256 PKCE and RS256 identity tokens is supported; no SAML or universal provider compatibility is claimed. Bind the provider's stable `(issuer, subject)` to an existing local account in administration or with `member identity-bind`. Email alone never links accounts or grants privileges.

The connector checks signature, issuer, audience, expiry, nonce, authorized-party and applicable access-token hash, and uses a separate secure browser-bound one-use state cookie. HTTPS transport verifies certificates, rejects redirects, times out in five seconds and bounds provider responses. Pending flows expire after five minutes and have a 1,000-flow global ceiling. They are not archived. Failed/expired/replayed sign-ins fail closed; local sign-in remains available independently. Removing a binding revokes that account's sessions, including password sessions. Test a configured provider in its sandbox before deploying it; deterministic JWT, browser-state and actual HTTPS adapter exchanges are separate from real-provider certification.

## Recovery and observability

M5 advances the shared schema/archive to version 8. Opening a schema-7 database adds M5 tables in the forward migration. Version-7 backup archives are intentionally not accepted by the version-8 importer; restore with the M4 executable first, then open that recovered database with M5 and produce a new archive. Do not keep an old archive parser as a permanent compatibility layer.

Fresh restore validates users, policies, resources, course editions, download ownership, progress, assignments, certificates, group relationships and identity bindings before writing. Pending provider handshakes and derived row counters are excluded; counters rebuild from restored rows. Backups include sensitive member work, password hashes and identity subjects; apply the existing private storage and trusted-backup policy.

Debug diagnostics retain SQL structure/timing and request/security outcomes without bound values, submission bodies, provider codes/tokens or secrets. Course publication and attempt records emit structured events with stable IDs, edition and completion state. Read database state and reviewable journey evidence alongside logs; a successful HTTP status alone does not prove policy or data integrity.

Development previews created from earlier draft M5 commits must be reset before using the final M5 schema: no public M5 deployment or archive compatibility is promised between draft checkpoints. The supported upgrade boundary remains merged M4 schema 7 to final M5 schema 8.
