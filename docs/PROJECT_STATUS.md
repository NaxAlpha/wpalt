# wpalt — Full project status and remaining work

Status date: 2026-10-28 (Asia/Tokyo). This document is the current overview; earlier milestone ledgers retain their historical source-specific observations. Product scope remains the [agreed goal](wpalt-product-goals.md), with detailed gaps in [the backlog](deferred-features.md) and [capability matrix](feature-parity.json).

## Overall picture

wpalt is an owner-operated Rust CMS with bundled administration and public rendering, SQLite/PostgreSQL, shared content/design/permission/job/recovery primitives and optional external connectors. M1–M9, M3.5 and defined follow-up packages D01–D11 are merged. D12 implements the defined offline/recovery system and closes through [PR28](https://github.com/NaxAlpha/wpalt/pull/28). Its final delivery requires successful PR checks, merge, actual-main gates and independently verified public release; the PR and [workflow runs](https://github.com/NaxAlpha/wpalt/actions/workflows/m1.yml) are the live delivery record. There is no claim that the entire WordPress-alternative goal is complete.

D13 is preparation only. D13–D30 are 18 remaining named packages, in addition to breadth retained from earlier packages. The 132 catalog groups are planning units ; 23 are optional connector/external boundaries. Counts do not give a meaningful percentage of full product completion.

## Delivered milestone systems

| Stage | End-user system | Delivery |
|---|---|---|
| M1 | Install/configure, content/media, drafts/revisions/scheduling, public site/search/feed/comments, manual recovery | [PR1](https://github.com/NaxAlpha/wpalt/pull/1) |
| M2 | Shared typed models, theme packages, reusable composition, bindings and visual Studio | [PR3](https://github.com/NaxAlpha/wpalt/pull/3) |
| M3 | Multilingual publication, metadata, search/discovery, redirects and structured outputs | [PR6](https://github.com/NaxAlpha/wpalt/pull/6) |
| M3.5 | Integrated structured block writing, organization, undo/recovery, previews and publication | [PR7](https://github.com/NaxAlpha/wpalt/pull/7) |
| M4 | Forms, responses, audience/consent, mail, local analytics and engagement workflows | [PR8](https://github.com/NaxAlpha/wpalt/pull/8) |
| M5 | Membership, protected resources, courses/progress/quizzes/assignments, moderated groups and certificates | [PR10](https://github.com/NaxAlpha/wpalt/pull/10) |
| M6 | Local commerce, stock/orders/receipts, purchase-derived access and capacity-bound reservations | [PR12](https://github.com/NaxAlpha/wpalt/pull/12) |
| M7 | Recovery/encryption/retention, native PostgreSQL PITR, protection, privacy and local resource controls | [PR13](https://github.com/NaxAlpha/wpalt/pull/13) |
| M8 | Selected WordPress/plugin migration, reviewed imports and scoped external integrations | [PR14](https://github.com/NaxAlpha/wpalt/pull/14) |
| M9 | Two PostgreSQL app processes plus worker on one Unix host, coordinated authority, stopped maintenance and cumulative releases | [PR16](https://github.com/NaxAlpha/wpalt/pull/16) |

Supporting deliveries include clean post-merge builds (PR2), frontend measurement foundations (PR4), calm cardless UI (PR5), cumulative adversarial review (PR9), automatic dated releases/parallel CI (PR11) and encoder-source build preparation (PR15). These are supporting changes, not extra product-completion claims.

## Follow-up package progress

| Package | Defined end-user outcome | Delivery |
|---|---|---|
| D01 | Classified directories, hierarchical taxonomy/archives and validated typed authoring | [PR17](https://github.com/NaxAlpha/wpalt/pull/17) |
| D02 | Assigned exact-material review, private queue/history and controlled publication | [PR18](https://github.com/NaxAlpha/wpalt/pull/18) |
| D03 | Connected language workspace, account interface language and resumable canonical translation proposals | [PR19](https://github.com/NaxAlpha/wpalt/pull/19) |
| D04 | Hierarchical language navigation, reusable styles/typography and owner-local font/theme bundles | [PR20](https://github.com/NaxAlpha/wpalt/pull/20) |
| D05 | Reviewed selected Elementor design migration into editable native composition with explicit losses | [PR21](https://github.com/NaxAlpha/wpalt/pull/21) |
| D06 | Discovery previews, local social images and publication-aware image/video/news sitemaps | [PR22](https://github.com/NaxAlpha/wpalt/pull/22) |
| D07 | Typed visible structured publication facts and multiple business locations | [PR23](https://github.com/NaxAlpha/wpalt/pull/23) |
| D08 | Redirect policy/preview, automatic slug redirects, bounded 404 reporting and rendered-link analysis | [PR24](https://github.com/NaxAlpha/wpalt/pull/24) |
| D09 | Connected editorial insights and durable bounded public graph analysis | [PR25](https://github.com/NaxAlpha/wpalt/pull/25) |
| D10 | Composable finite conditions/calculations, native scalar collection and frozen cohort reporting | [PR26](https://github.com/NaxAlpha/wpalt/pull/26) |
| D11 | Optional local private scanning, frozen printable/PDF responses and independently verified server attestations | [PR27](https://github.com/NaxAlpha/wpalt/pull/27) |
| D12 | Deliberate public-text/scalar-form offline library and separately confirmed partial-response recovery | [PR28](https://github.com/NaxAlpha/wpalt/pull/28) |

Each row is its contract-defined package, not complete free/premium plugin parity. Contracts and evidence are under `docs/dNN-contract.md` and `docs/evidence/dNN-verification.md`.

## D12 closure boundary

Visitors deliberately save anonymous public article text/eligible scalar forms, reopen/edit without a connection, then reconnect for explicit review. The `/offline/` worker uses release-asset hashes, anonymous fetches, origin-wide Web Locks and finite storage/expiry. It does not cache authenticated page HTML or send responses automatically. Changed publications preserve original values and unresolved request identity. Repeated recognition of an already accepted original response creates no duplicate entry.

Owners publish a specific recovery purpose and selected safe fields. Owner-visible partial answers require a separate visitor request and mailbox confirmation; no covert exit capture or inferred marketing permission. Withdrawal, expiry, matching completion and changed publication/policy fence queued reminders. Limits: 512 KiB per public snapshot, 30 items / 2 MiB total, seven days; recovery selects 1–16 fields / 16 KiB values, seven days and at most 1,000 records per form within shared draft quotas. Native 25 / portable 20 uses stopped upgrade and authentic original D11 rollback/conversion/fresh recovery.

Closure also updates the security dependencies to Ammonia 4.2.4, Rustls 0.23.46, rustls-webpki 0.103.16, webpki-roots 1.0.10 and Tokio 1.53.3. The 2026-10-28 selected-graph audit reports zero active vulnerabilities/warnings; one inactive optional RSA lockfile advisory remains distinguished ([receipt](evidence/d12-closure-dependency-audit.json)). This is a dated audit, not a promise of future vulnerability absence. No advisory gate was relaxed.

## Current verification, size and performance evidence

| Measure | Recorded result and scope |
|---|---|
| Rust application | 133 tracked files; 49,278 physical lines in `src/` |
| Rust test source | 14 tracked files; 21,094 physical lines in `tests/` |
| Frontend directory | 23 tracked files; 10,258 physical lines, including manifests/lock data |
| Verification scripts | 84 tracked files; 11,628 physical lines |
| Rust scenarios | 148 test annotations/scenarios; relevant cases loop across SQLite and required real PostgreSQL |
| Pipeline safety | 13 Python pipeline tests; separate from Rust scenarios |
| Current coverage percentage |Not measured for this source. Historical instrumentation is tied to its old source; test counts do not establish line/branch coverage |
| D12 UI evidence |12 measured layouts at 320/768/1440, four scoped axe audits, real offline close/reopen/edit/reload, original interrupted retry, capacity and failure journeys |
| Native-form regression |15 measured layouts/five scoped audits; JavaScript-disabled canonical calculator submission and pinned free reference execution |
| Local process evidence |Two app processes and one worker, PostgreSQL, round-robin without sticky routing/retry; same-host correctness only |
| D12 snapshot observation |One three-field public form: 1,475 bytes ; 30 serial local debug HTTP requests, median 3.567 ms / p95 11.911 ms; earlier exact executable in [receipt](evidence/d12-local-browser.json) |
| Earlier optimized footprint |D11 Linux executable 41,153,544 bytes; warm app RSS 49,872,896 bytes (SQLite) / 45,162,496 (PostgreSQL); excludes DB/scanner/browser workers and is not peak memory |
| Earlier populated workload |D11: 1,000 accepted acknowledgments/engine; SQLite DB/WAL/SHM 7,466,920 bytes, PostgreSQL relations/indexes 9,797,632 bytes; serial HTML median 1.456 / 2.676 ms, p95 1.909 / 2.870 ms, five SELECTs |

Line inventory is from the unchanged application source at checkpoint 77ab007; counts include blank lines/comments, are not logical LOC and are not productivity/quality scores ([inventory](evidence/d12-code-inventory.json)). Dependency/documentation closure changes do not alter those source counts. D11 resource measurements are historical and **must not be presented as the new closure binary's footprint**; [exact identities/conditions](evidence/d11-main-release-document-observations.json) and [M9 workload evidence](evidence/m9-final-resources.md) remain available. No current peak-memory, full-site capacity, physical-host, TLS throughput, WordPress efficiency, field CWV or SLA claim.

Seven required gates remain: application, clean-build, compiler-floor, dependency-audit, frontend, native-pitr and local-processes. Clean-build uses empty Cargo/target directories and tests the actual packaged executable; browser and process jobs independently verify/download it. Main publication waits for every gate. Dated Linux development prereleases include `BUILD.json`, source/lock/assets identities and checksums. Publication is not deployment; operators explicitly maintain/upgrade databases. Evidence freshness and migration/compatibility checks are release inputs, not vendor runtime account requirements.

## Security and data-integrity foundations

Existing protections include Argon2id passwords, hashed random sessions/capabilities, CSRF plus Origin checks, HTTPS secure cookies, local authenticator/passkey support, finite authentication/admission budgets, current role/resource checks and private no-store output. Shared validated document/schema/composition grammars avoid arbitrary owner scripts as a public execution mechanism. Uploads have bounded type/size admission; optional local scanning is fail-closed but does not establish complete malware detection. Server signatures attest exact artifact bytes, not certified respondent identity.

Optimistic versions, transactional state/quota/idempotency, explicit consent/publication snapshots and queue delivery preflight protect interacting workflows. Native graph validation and encrypted independently retained recovery points support fresh restores. Debug events expose request/SQL/job timings and static security outcomes while excluding secrets/bound payload values. Same-host multi-process lifecycle/coordination prevents unreviewed stale mutations and pauses uncertain work; it is not distributed automatic failover or exactly-once external delivery.

Custom role authoring, stronger process isolation, complete privacy-erasure identity breadth and physical-host behavior remain open below. Security mechanisms and successful tests do not prove exhaustive absence of bugs.

## Remaining named packages

| Package | Area | Concrete remaining work |
|---|---|---|
| D13 | Custom authorization | Owner-authored roles/capabilities with consistent administrative/API/content/worker policy enforcement. |
| D14 | Learning and community breadth | Broader assessments/assignment attachment/editor workflows, richer course administration and community features beyond versioned multiple-choice/text/private discussion. |
| D15 | Affiliate attribution | Automatic reviewed purchase-linked commissions and payout bookkeeping beyond local referral counts/manual commissions. |
| D16 | Commerce breadth | Guest checkout, additional currencies/pricing/shipping/tax models, richer refund/fulfillment/provider workflows and broader booking calendars beyond current defined store scope. Authoritative catalog price/currency/availability/offers enrichment for structured output remains here; D07 editorial declarations deliberately refuse those properties. |
| D17 | Provider certification | Actual merchant-account sandbox/end-to-end payment verification; current HTTPS Stripe adapter fixture is not an actual Stripe-account certification. |
| D18 | Analytics and funnels | Arbitrary native funnel builder, purchase/conversion joins and expanded retained-data reporting. |
| D19 | Engagement breadth | Exit-intent triggering, richer campaign UI and explicitly reviewed offer presentation/redemption workflows beyond accessible native targeted dialogs and current claims. |
| D20 | Campaign automation | General visual automation graph and broader queue/segment workflow controls beyond confirmation/manual/scheduled templates. |
| D21 | Shared cache and replicas | Optional external shared object-cache and explicit read-replica consistency/route policy. |
| D22 | Maintenance and media isolation | Explicit safe database compaction, broader bounded video profiles and stronger owner-hosted codec/signature isolation and resource accounting. |
| D23 | Privacy breadth | More explicit consent categories/script contracts, independently verified anonymous/outside-data requests and wider finite site scanning. |
| D24 | Migration breadth | Expand selected WordPress/plugin export adapters and media/style/relationship mappings; stronger resumable assessment tools and reviewed larger source workloads. |
| D25 | Extensions and snippets | Broader developer-hook API/ABI and an explicit isolated approved-snippet/runtime contract. Dynamic module loading/unloading is an option to evaluate, not an existing supported feature. |
| D26 | Versioned lifecycle | More supported configuration/schema/theme/integration upgrade paths, executable replacement orchestration and compatibility-removal automation. |
| D27 | Fleet operations | Bulk reviewed fleet upgrade/repair, durable external scheduling/alerts and independent fleet operator UI beyond the read-only native inspector. |
| D28 | Deployment | Physical multi-host operation, shared storage/fencing, horizontal throughput and automated failover/rolling strategy. |
| D29 | Frontend/accessibility | Broader browser/device/IME/assistive-technology and large-document support beyond tested Chromium geometry/keyboard/text-spacing/RTL journeys. Real-time coediting and Notion database/workspace parity need a separate product decision. |
| D30 | Adoption | Public license, version/support/deprecation policy, release signing/SBOM and comprehensive instrumented frontend/CLI coverage if adopted. |

D13 has a [source/authority assessment](evidence/d13-preparation/authorization-assessment.md), lexical callsite inventory and pinned free-source provenance; no D13 application implementation exists. Complete and verify the current release before starting its separate delivery PR.

## Breadth retained from completed packages

- D01: additional field families/custom validation/relationships beyond the current grammar.
- D02: configurable multi-stage/multi-approver policies, larger reviewer search and custom contributor permissions.
- D03: full professional screen/localization catalogs, locale breadth and demonstrated model translation quality. Actual earlier Qwen French output failed editorial quality; generated proposals remain private drafts requiring correction.
- D04: mega-menu templates, additional font formats/variable/color fonts, broader assets/budgets and pruning workflows.
- D05: complete Elementor widgets/templates/styles/dynamic/display conditions and rendered-output migration. Selected mappings and inspectable losses do not imply PHP/runtime or pixel parity.
- D06–D09: broader theme/dynamic graph coverage, ontology/provider breadth and language-sensitive editorial analysis beyond current selected profiles/advisories.
- D10: modulo/power and broader mathematics, matrix/ranking/specialized statistics, richer reporting and native private-widget breadth.
- D11: certified respondent identity, qualified/legal/PAdES signatures, actual external identity interoperability and stronger worker isolation/intelligence operations.
- D12: broader offline site/media/navigation/synchronization and private/account/attachment forms. Offline revocation cannot be discovered while disconnected; seven-day copies may be evicted by the browser.

These remain requirements to assess and deliver; closing a defined package never silently deletes them. The detailed backlog and 132-group matrix remain authoritative.

## External help and owner decisions

Most remaining local engineering can proceed without accounts. Actual payment-provider certification requires a genuine authorized merchant sandbox/account; production SMTP/OIDC/payment connections need owner credentials; real physical-host/independent-storage/failover verification needs actual infrastructure. Public license, support/version/deprecation commitments and release trust/signing decisions need owner alignment. No purchase or public support commitment is inferred from repository visibility.

Global CDN/DDoS absorption, proprietary search/spam/intelligence datasets, payment networks, global email deliverability and legally certified identity are external capabilities, not origin-server features to fabricate. Connectors remain optional. Real-time coediting/Notion database-workspace parity require a separate product decision.

## How to continue

Start with [README](../README.md), this status, [resume/setup guide](RESUME.md), [methodology](wpalt-development-methodology.md), [D13 preparation](evidence/d13-preparation/authorization-assessment.md) and [the backlog](deferred-features.md). Refresh affected primary guidance at implementation time. Deliver each next substantial user-facing package with readable component/cluster/end-to-end evidence, measured resources and security/race/recovery checks; use required SQLite/PostgreSQL and the exact-source seven-gate/release process. Do not restart old milestones or add compatibility shims instead of explicit stopped migrations.
