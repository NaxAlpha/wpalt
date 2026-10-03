# M5 — Membership and learning platform

Status: active implementation, after review PR #9 merged as `e7e58745a0de07b8a0af3814152fdaaf8c2fec21`. No completion claim until the current-source delivery gates pass.

## End-user delivery

Operate a private community/learning site through the shared admin, accounts, content editor and theme renderer. Members receive locally assigned, time-bounded entitlements and groups; protected publications and downloads obey the same policy. Operators build ordered courses from published content, define quizzes and assignments, review work and progress, and issue locally verifiable completion certificates. Members manage a profile, join protected discussion and retain learning state after restart/recovery. M6 supplies paid checkout/renewal/refund integration; M5 never fabricates successful purchases.

## Functional steps and defined scope

1. Identity/access: retain M4 verified registration/owner approval and established roles; add member profile editing, reusable policies requiring an entitlement and/or group, local grants/revocation/expiry, content/media rule administration and absolute/relative drip unlocks. Staff administration remains role guarded. Do not confuse theme visibility with authorization.
2. Learning: draft/live versioned courses, ordered lessons referencing shared content, authored multiple-choice quizzes, bounded attempts and idempotent concurrent completion, textual assignments with owner grading, current-version gradebook and completion certificates. Published-course revision is explicit; prior-version progress remains history and does not silently satisfy revised lessons.
3. Community/organization: protected group discussions, moderated member contributions and private profiles, delegated bounded organization seats and single-use local entitlement gifts. Gifts confer the issuer's predefined entitlement, never a caller-selected privilege.
4. Referral/integration: local referral attribution and manually recorded/voidable commission records; purchase-derived commissions and payment settlement belong to M6. Optional enterprise/social identity is an explicit configured provider integration, with local accounts continuing independently; provider credentials cannot be prerequisites for local learning.
5. Whole-system: protect pages/APIs/media/comments, public search/feed/sitemap/related content, jobs, export/recovery and module-disabled behavior. Member output is private/no-store/noindex. Protected records are excluded from anonymous discovery at SQL selection time. Recheck authoritative policy on every operation; revoked/expired/disabled users fail closed.

## Policy and operating boundaries

One owning process per site's data directory remains the supported runtime. SQLite and PostgreSQL are real verification targets. All mutation entry points require role/session/CSRF or the explicitly defined opaque single-use capability. No user-supplied executable theme/code, remote quiz grader, vendor login or mandatory internet service. External payment/provider behavior is tested only with an adapter/sandbox whose evidence is recorded; unavailable real credentials are not grounds for a fabricated verification claim.

A protected media reference must be protected explicitly as a resource. Admin workflows make that choice visible; publicly downloadable media is never made secret merely by embedding it in a protected page. Course lessons must use the course's policy and unlock delay, preventing direct lesson URLs from bypassing the learning plan. Policies cannot be deleted while referenced. Course/body revisions, grants and assignments preserve audit/history and use optimistic versions or transactional uniqueness. Public catalog output cannot reveal private lesson titles/body/answers.

## Verification outputs

Readable journeys cover two members with differing entitlements/groups, exact expiry/drip boundaries, grant/revoke/role changes, direct pages/API/media/discovery exclusion, quiz/assignment completion, concurrent/idempotent attempts, admin intervention, course republication, discussions/organization gifts/referral flows, restart and fresh restore, module-disabled fail-closed behavior. Real browser journeys cover admin authoring and learner success/error states at 320/768/1440, keyboard/text spacing, with screenshot review. Populated permission/dashboard/report queries record counts/plans and memory/latency observations without noisy timing assertions.

Deliver a milestone PR, migration notes, guidance/parity mappings, clean executable and current-source SQLite/PostgreSQL/browser/CLI CI evidence. No exhaustive plugin, enterprise identity provider or formal security/accessibility certification is implied.

## Implementation limits to verify explicitly

Course downloads are assigned to a specific lesson and share its direct-request prerequisites. Removing a lesson/download from a published course keeps its old resource protected until an owner deliberately changes that policy; publication never silently makes files public. Completed assessment work requires an operator reset before a new submission. Resets and requested changes revoke that edition's existing certificate; revoked certificates remain historical records and cannot automatically regain validity. UTC controls are explicit in native administration; CLI timestamps are Unix seconds.

The derived membership record counter covers all persisted membership tables, including history. `membership_max_records` defaults to 1,000,000. Database triggers enforce inserts/deletes transactionally, including recovery and upserts. Lowering the limit below existing usage permits reads and deletion while preventing further inserts; it does not discard data. Pending identity handshakes have a separate 1,000-flow, five-minute bound and are excluded from backups. Removing an identity binding revokes the associated account's current sessions.
