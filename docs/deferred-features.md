# Deferred feature backlog across M1–M9

## Decision and purpose

On 2026-10-06 (Tokyo), the owner directed us to document all remaining missing features for later, make the current M9 PR ready and merge it. This explicitly defers the remaining catalog breadth; it does not erase the original WordPress-alternative goal or convert defined slices into full plugin parity. Current native features, verified local-process deployment and stopped maintenance are the accepted delivery boundary. License remains undecided; merging is authorized independently of adopting a public support/license policy.

Companions: [catalog matrix](feature-parity.json), [full reconciliation](evidence/m9-capability-reconciliation.md), [product goals](wpalt-product-goals.md), [methodology](wpalt-development-methodology.md), [M9 contract](m9-contract.md).

## How to resume

Choose a substantial end-user system from this backlog, reassess against the latest code and current primary guidance, define concrete functional and failure journeys, then implement/verify/optimize it and deliver a separate PR. Source versions and paid/free access determine what can actually be compared. Do not count historical M1 limits as still missing where a later milestone already supplies them. Use each later contract/matrix coverage field as the starting evidence. Performance, security, recovery, migration and UI gates apply to every resumed feature. The owner subsequently authorized autonomous dependency-ordered resumption on 2026-10-06; see [decision 0015](decisions/0015-resume-local-backlog.md). External accounts/keys and owner-only adoption decisions remain separate. No new paid-provider purchase is authorized.

## Concrete deferred work

### D01 — Content models and taxonomy

Origin: **M1–M2** · Catalog: **F001/F002** · Status: **classified-directory scope verified in PR #17; broader field-family gaps retained**

Active [classified-directory contract](d01-contract.md), [verification ledger](evidence/d01-verification.md), [PR #17](https://github.com/NaxAlpha/wpalt/pull/17).

Hierarchical taxonomy/archives, expanded field families and custom validation/relationships beyond the supported typed grammar.

Verifiable future outcome: Create hierarchical content, edit typed values and preserve archive URLs/relationships across publication and recovery.

### D02 — Editorial workflow

Origin: **M1–M2** · Catalog: **F007/F021** · Status: **defined assigned-review system merged in PR #18; actual-main release independently verified**

Active [editorial-workflow contract](d02-contract.md), [approval decision](decisions/0016-editorial-approval-boundary.md) and [free reference investigation](evidence/d02-reference.md), [verification](evidence/d02-verification.md) and [PR #18](https://github.com/NaxAlpha/wpalt/pull/18).

Assigned exact-material review, private actionable queue/notes, bounded history and publication/schedule authority are verified in D02. Configurable multi-stage/multi-approver policy, large-team reviewer search and custom contributor capabilities remain explicit future breadth; external email transport is optional owner configuration.

Verifiable future outcome: An author requests review; an authorized reviewer approves/rejects; stale approval cannot publish changed work.

### D03 — Languages and owner usability

Origin: **M1–M3** · Catalog: **F008–F011** · Status: **defined language-workspace delivery merged in PR #19; broader localization/model quality remains**

[D03 contract](d03-contract.md) defines the connected translation/localization system.

D03 delivers scoped durable interface languages, connected saved-variant comparison/copy/synchronization and source-bound resumable canonical text proposals. Complete professional screen catalogs, arbitrary locale/format breadth, provisioned protected-target model application and demonstrated high-quality translation remain. Actual Qwen French quality fails editorial review; generated output remains a private draft requiring correction.

Verifiable future outcome: Edit and compare language variants in the UI; no translated publication, privacy rule or source edit is overwritten silently.

### D04 — Theme composition and assets

Origin: **M2/M8** · Catalog: **F016/F019** · Status: **defined local theme system implemented; final delivery gates pending**

[D04 contract](d04-contract.md) governs composed publication and local asset authority.

D04 adds language-specific hierarchical/grouped navigation, reusable controlled styles/typography, shared owner-local static TrueType fonts and atomic private theme bundles. Overlay mega-menu templates, broader font containers (WOFF/WOFF2/CFF/variable/color/SVG), arbitrary raw CSS/executable insertion, configurable asset ceilings and manual history pruning remain. Full Elementor adaptation stays D05; isolated executable extension authority stays D25. [Verification](evidence/d04-verification.md) records exact scope and failures.

Verifiable future outcome: An independent theme preserves responsive/accessibility contracts and asset isolation under explicit owner trust.

### D05 — Elementor compatibility

Origin: **M8** · Catalog: **F024** · Status: **queued for dependency-ordered autonomous resumption**

Full template/widget/style/dynamic/display-condition mapping and rendered-output compatibility. Current heading/text projection plus explicit losses remains supported.

Verifiable future outcome: Selected real free/premium source fixtures preserve reviewed layout/content; unsupported widgets and migration losses remain inspectable. No PHP runtime parity is assumed.

### D06 — SEO previews and sitemap extensions

Origin: **M3** · Catalog: **F025/F027** · Status: **queued for dependency-ordered autonomous resumption**

Social image/SERP-preview management and image/video/news sitemap extensions.

Verifiable future outcome: Published/private boundaries, latest primary guidance and meaningful generated-output tests cover each extension; no ranking promise.

### D07 — Structured data and locations

Origin: **M3** · Catalog: **F028/F029** · Status: **queued for dependency-ordered autonomous resumption**

Custom schema graph builder, richer supported rich-result types and multiple business locations.

Verifiable future outcome: Owner-authored visible entities produce validated publication-only structured output with guidance/version checks.

### D08 — Redirects and link analysis

Origin: **M3** · Catalog: **F030/F032** · Status: **queued for dependency-ordered autonomous resumption**

Bounded regex rules, safe automatic slug redirects, actionable 404 reporting and graph coverage of theme/navigation/raw HTML/dynamic links.

Verifiable future outcome: Preview loops/conflicts and work budgets; a complete claimed graph includes documented edge sources without unauthorized external fetching.

### D09 — Editorial tooling

Origin: **M3/M9** · Catalog: **F031/F034** · Status: **queued for dependency-ordered autonomous resumption**

Administration screens for native link/orphan/text/keyword tools, richer language-aware advisory analysis and larger incremental graph workloads.

Verifiable future outcome: An editor reviews useful suggestions alongside content, understands omissions and receives no fabricated ranking score.

### D10 — Advanced forms and surveys

Origin: **M4** · Catalog: **F038/F042/F044** · Status: **queued for dependency-ordered autonomous resumption**

Richer declarative conditional/calculation grammar and specialized survey reporting beyond bounded presence/equality/sum/product/choice score.

Verifiable future outcome: Server and UI agree on calculations and visibility; expressions cannot execute owner/server code or alter reviewed pricing.

### D11 — Attachment and signature workflows

Origin: **M4** · Catalog: **F041/F043** · Status: **queued for dependency-ordered autonomous resumption**

Optional isolated malware scanning, cryptographic signature/identity integrations and generated signed-document artifacts.

Verifiable future outcome: Untrusted files remain private/quarantined; identity and cryptographic claims require real provider/standard evidence.

### D12 — Offline and resumable workflows

Origin: **M4** · Catalog: **F045/F046** · Status: **queued for dependency-ordered autonomous resumption**

Broader offline site navigation/synchronization and explicit consented abandonment workflows beyond already-loaded form retry and partial drafts.

Verifiable future outcome: Conflict-aware online reconciliation preserves original reviewed payload and consent; no covert tracking.

### D13 — Custom authorization

Origin: **M5** · Catalog: **F051** · Status: **queued for dependency-ordered autonomous resumption**

Owner-authored roles/capabilities with consistent administrative/API/content/worker policy enforcement.

Verifiable future outcome: Least-privilege custom roles cannot bypass direct-resource policy or retain access after revocation.

### D14 — Learning and community breadth

Origin: **M5** · Catalog: **F055/F056/F058** · Status: **queued for dependency-ordered autonomous resumption**

Broader assessments/assignment attachment/editor workflows, richer course administration and community features beyond versioned multiple-choice/text/private discussion.

Verifiable future outcome: Edition changes preserve historical progress; prerequisites and moderation apply to direct content/download/API routes.

### D15 — Affiliate attribution

Origin: **M5–M6** · Catalog: **F059** · Status: **queued for dependency-ordered autonomous resumption**

Automatic reviewed purchase-linked commissions and payout bookkeeping beyond local referral counts/manual commissions.

Verifiable future outcome: Idempotent orders/refunds/reversals reconcile commission authority; outside money transfer remains external.

### D16 — Commerce breadth

Origin: **M6** · Catalog: **F060–F069** · Status: **queued for dependency-ordered autonomous resumption**

Guest checkout, additional currencies/pricing/shipping/tax models, richer refund/fulfillment/provider workflows and broader booking calendars beyond current defined store scope.

Verifiable future outcome: Concurrent stock/capacity, checked money, stale totals, revocation, repeat receipts and recovery remain correct under every added workflow.

### D17 — Provider certification

Origin: **M6** · Catalog: **F069** · Status: **queued for dependency-ordered autonomous resumption**

Actual merchant-account sandbox/end-to-end payment verification; current HTTPS Stripe adapter fixture is not an actual Stripe-account certification.

Verifiable future outcome: Real authorized test account records verify payment/subscription/refund/cancellation and ambiguous-response handling without public credentials.

### D18 — Analytics and funnels

Origin: **M4/M6** · Catalog: **F085/F086** · Status: **queued for dependency-ordered autonomous resumption**

Arbitrary native funnel builder, purchase/conversion joins and expanded retained-data reporting.

Verifiable future outcome: Consent, withdrawal and authoritative purchase facts govern reports; bounded queries preserve private fields and financial history.

### D19 — Engagement breadth

Origin: **M4** · Catalog: **F089/F091** · Status: **queued for dependency-ordered autonomous resumption**

Exit-intent triggering, richer campaign UI and explicitly reviewed offer presentation/redemption workflows beyond accessible native targeted dialogs and current claims.

Verifiable future outcome: Keyboard/assistive-device behavior and delayed withdrawal/regrant responses stay safe; cookie allocation is not identity/fraud proof.

### D20 — Campaign automation

Origin: **M4** · Catalog: **F098** · Status: **queued for dependency-ordered autonomous resumption**

General visual automation graph and broader queue/segment workflow controls beyond confirmation/manual/scheduled templates.

Verifiable future outcome: Versioned consent/revocation and durable receiver-aware idempotency prevent stale or duplicate unwanted actions.

### D21 — Shared cache and replicas

Origin: **M7** · Catalog: **F074/F122** · Status: **queued for dependency-ordered autonomous resumption**

Optional external shared object-cache and explicit read-replica consistency/route policy.

Verifiable future outcome: Revoked/private/withdrawn data cannot reappear under replica lag; retain owner-local operation and fail-closed sensitive writes.

### D22 — Maintenance and media isolation

Origin: **M7** · Catalog: **F080/F082/F106** · Status: **queued for dependency-ordered autonomous resumption**

Explicit safe database compaction, broader bounded video profiles and stronger owner-hosted codec/signature isolation and resource accounting.

Verifiable future outcome: Interruption retains source/history; resource pressure and stale authorization cannot publish or destroy media.

### D23 — Privacy breadth

Origin: **M7** · Catalog: **F109/F110/F111** · Status: **queued for dependency-ordered autonomous resumption**

More explicit consent categories/script contracts, independently verified anonymous/outside-data requests and wider finite site scanning.

Verifiable future outcome: GPC/DNT/withdrawal, exact purpose/version grants and proven subject identity apply; financial/audit retention stays explicit.

### D24 — Migration breadth

Origin: **M8** · Catalog: **F001–F132 adapters** · Status: **queued for dependency-ordered autonomous resumption**

Expand selected WordPress/plugin export adapters and media/style/relationship mappings; stronger resumable assessment tools and reviewed larger source workloads.

Verifiable future outcome: Use actual versioned source fixtures; report losses; never synthesize credentials, access, consent or settlement. Paid-only evidence is distinguished from free reference tests.

### D25 — Extensions and snippets

Origin: **M9** · Catalog: **F124/F129** · Status: **queued for dependency-ordered autonomous resumption**

Broader developer-hook API/ABI and an explicit isolated approved-snippet/runtime contract. Dynamic module loading/unloading is an option to evaluate, not an existing supported feature.

Verifiable future outcome: Versioned grants and revocation work under resource limits; arbitrary code does not inherit CMS/server privilege or bypass native authority.

### D26 — Versioned lifecycle

Origin: **M9** · Catalog: **F125/F128** · Status: **queued for dependency-ordered autonomous resumption**

More supported configuration/schema/theme/integration upgrade paths, executable replacement orchestration and compatibility-removal automation.

Verifiable future outcome: Old meaningful data migrates or fresh rollback recovers; finite compatibility bridges have owner/version/removal criteria.

### D27 — Fleet operations

Origin: **M9** · Catalog: **F126/F127/F130** · Status: **queued for dependency-ordered autonomous resumption**

Bulk reviewed fleet upgrade/repair, durable external scheduling/alerts and independent fleet operator UI beyond the read-only native inspector.

Verifiable future outcome: A failed origin does not hide others; least-privilege grants and stopped-site recovery govern updates. Exactly-once arbitrary external effects is not promised.

### D28 — Deployment

Origin: **M9** · Catalog: **F074/F122/F126–F130** · Status: **queued for dependency-ordered autonomous resumption**

Physical multi-host operation, shared storage/fencing, horizontal throughput and automated failover/rolling strategy.

Verifiable future outcome: Run genuinely separate hosts, partition/kill ownership and test stale writers, media/config consistency, worker retries and revocation before enabling the topology.

### D29 — Frontend/accessibility

Origin: **M3.5/M9** · Catalog: **Editor/core/frontend** · Status: **queued for dependency-ordered autonomous resumption**

Broader browser/device/IME/assistive-technology and large-document support beyond tested Chromium geometry/keyboard/text-spacing/RTL journeys. Real-time coediting and Notion database/workspace parity need a separate product decision.

Verifiable future outcome: Record actual device/browser/AT evidence; use understandable complete editing and recovery journeys rather than snapshot count inflation.

### D30 — Adoption

Origin: **M9** · Catalog: **Release/core** · Status: **queued for dependency-ordered autonomous resumption**

Public license, version/support/deprecation policy, release signing/SBOM and comprehensive instrumented frontend/CLI coverage if adopted.

Verifiable future outcome: Owner chooses license/support; independently verified releases, migration paths and scoped coverage reports accompany claims.

## External boundaries and deliberate constraints

The 23 catalog entries classified optional connector/external boundary remain optional. Global CDN/DDoS absorption, proprietary ranking/vulnerability/spam intelligence, card settlement, carrier/tax/legal datasets, global deliverability, vendor translation networks and independent off-site storage are not missing origin-server implementations to fabricate. A remote service adapter, independent worker or external host may be chosen later, but no vendor account is required for existing native operation. F069 additionally has an optional adapter with unverified real-provider-account certification.

Fixed work/size/time limits, explicit private authorization, exactly-once-effect caveats and fresh-target restore are supported safety contracts, not bugs or promises to remove later. Change them only with meaningful evidence and migration. Local scripts/model/FFmpeg are optional separate owner-managed processes; provider/dataset/codec quality and OS isolation are not guaranteed by the CMS.

## Complete capability coverage snapshot

This appendix preserves every group’s current defined coverage, including groups with no additional work presently identified. A coverage description records a slice, not premium-product equivalence. D01–D30 capture the actionable deferred families; the catalog remains the source for finer future decomposition. Original source/version references remain in the matrix and research catalog.

- **F001 · Custom types and taxonomies** (M2, deployment L): Installed custom models and model taxonomies; shared draft/live term joins, schema preflight and structured authoring. Taxonomy hierarchy/configurable taxonomy archives remain later refinements.

- **F002 · Typed custom fields and relationships** (M2, deployment L): Shared/model primitive, media and relationship fields; required/type/target validation and batched published relationship resolution. Not every ACF/Pods field type is claimed.

- **F003 · Repeaters, galleries and clone groups** (M2, deployment L): Bounded nested repeaters, image galleries and reusable field groups; group reuse shares definitions rather than copying records.

- **F004 · Flexible content / reusable block schemas** (M2, deployment L): Flexible typed variant/group sections, nested values and reusable parameterized compositions. Arbitrary custom block JavaScript is outside the graph.

- **F005 · Global options / settings pages** (M2, deployment L): Shared typed options with independent draft/live snapshots, authoring widgets and explicit publication.

- **F006 · Content revisions and rollback** (M2, deployment L): Content revisions preserved; bounded theme revisions restore to a draft with optimistic concurrency. Schema changes need explicit data migrations.

- **F007 · Drafts, preview and editorial approval** (M2, deployment L): Private content/theme previews and safe publication; editorial approval policy is later workflow work.

- **F008 · Manual language variants and strings** (M3, deployment L): Independent language variants, labels/direction and per-language navigation. No automatic translation or admin localization.

- **F009 · Translation duplication / synchronization** (M3, deployment L): Stopped-site exact-plan grouped draft duplication and explicitly selected field synchronization, preserving canonical translated document/publication/access; no unattended overwrite.

- **F010 · Multilingual slugs and metadata** (M3, deployment L): Configured language slugs, publication-only localized canonical/hreflang, reciprocal published variants and draft exclusion; no automatic translation claim.

- **F011 · Automatic translation with local models** (M3, deployment W): Independent loopback model title/body proposals for short passages, native configured-language grouped unscheduled drafts and source-bound review. Actual language-quality evidence remains active; no universal accuracy claim.

- **F012 · Vendor automatic translation / human network** (M3, deployment E): Multilingual behavior belongs to the agreed M3 discovery platform; requirement retained.

- **F013 · Responsive blocks and layout primitives** (M2, deployment L): Responsive grid/row/stack, controlled spacing/width/colors, semantic heading levels and desktop/mobile studio previews; no arbitrary CSS execution.

- **F014 · Theme-wide templates and dynamic field bindings** (M2, deployment L): Home/search/content/model templates, typed/dynamic bindings, model collections and repeater/relationship rendering through one server engine.

- **F015 · Reusable patterns and design tokens** (M2, deployment L): Parameterized reusable components and six global color/font tokens; no remote pattern marketplace required.

- **F016 · Header, footer and navigation builder** (M2, deployment L): Compose shared header/footer/navigation nodes; navigation items use local site settings. Mega-menu/hierarchical navigation is not claimed.

- **F017 · Tabs, accordions, galleries and carousels** (M2, deployment L): Keyboard progressive tabs, native details accordion, gallery and horizontally scrollable scroll-snap carousel. No automatic carousel rotation or full WCAG certification.

- **F018 · Conditional / protected presentation** (M2, deployment L): Bounded scalar/comparison/all/any/not predicates on public/typed contexts; conditional visibility does not grant membership authorization (M5).

- **F019 · Custom CSS and approved asset insertion** (M2, deployment L): Controlled responsive styles/tokens and permission-aware uploaded images; freeform CSS and executable asset insertion remain M8 extension work.

- **F020 · Template import/export and cross-site reuse** (M2, deployment L): Portable declarative JSON import/export in studio and offline CLI; validates installed model/field/component dependencies, does not silently bundle a site database/media.

- **F021 · Collaborative notes and assignments** (M4, deployment L): Versioned private response notes and active-editor assignments; not real-time shared editing.

- **F022 · Cloud template marketplace/library** (M2, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F023 · AI layout/content generation locally** (M8, deployment W): examples/integrations/local_ai_worker.py; scoped API read/draft; actual Ollama 0.35.1/Qwen3 0.6B content and bounded native-theme layout workflow; scripts/local_ai_reference.py

- **F024 · Exact Elementor template/widget compatibility** (M8, deployment L): src/platform/elementor.rs; explicit WXR --elementor-content; actual free Elementor 4.3.3 saved document/export reconciliation passed at 17fb901; unimplemented widgets/style details remain explicit losses

- **F025 · SEO titles, metadata and social previews** (M3, deployment L): Per-content title/description and social metadata; no social image manager or SERP preview scoring.

- **F026 · Canonical, robots and indexing rules** (M3, deployment L): Local canonical, noindex, robots and reciprocal published hreflang; no arbitrary external canonical override.

- **F027 · XML, image, video and news sitemaps** (M3, deployment L): Bounded XML content sitemaps and index; image/video/news extensions deferred.

- **F028 · Schema graph and custom schema builder** (M3, deployment L): One minimal WebPage/Article graph with optional LocalBusiness node; arbitrary schema builder and rich-result enrichment deferred.

- **F029 · Local business / multiple location schema** (M3, deployment L): One visible business identity/address; multiple locations and richer business types deferred.

- **F030 · Redirects, regex rules and 404 reports** (M3, deployment L): Exact local 301/302 rules with loop/conflict/work limits; regex, automatic slug redirects and 404 reports deferred.

- **F031 · Internal-link graph / orphan detection** (M3, deployment L): Bounded complete public Markdown content-link graph with redirects, deduplicated non-self edges and inbound-link candidates; navigation/theme/dynamic graphs excluded.

- **F032 · Internal broken-link checks** (M3, deployment L): Bounded published Markdown local link/public media checks, no external fetching; theme/raw HTML crawler extensions deferred.

- **F033 · External broken-link checks** (M3, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F034 · Readability and keyword heuristics** (M3, deployment L): Advisory published-text whitespace/sentence/paragraph/keyword counts; language-sensitive, no ranking grade, preferred length or density target.

- **F035 · Search Console / search index data** (M3, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F036 · SERP ranking / competitive keyword datasets** (M3, deployment I): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F037 · Form designer and reusable field schema** (M4, deployment L): Visual typed field/step designer, shared groups/repeaters, immutable publications and canonical content/theme form embedding.

- **F038 · Conditional fields / notification routing** (M4, deployment L): Earlier-field visibility and fixed-recipient canonical notification rules using presence/scalar equality; no arbitrary executable expressions.

- **F039 · Multi-step forms and repeatable inputs** (M4, deployment L): Eight ordered steps and bounded shared nested/repeated input grammar.

- **F040 · Entry storage, search, CSV export and permissions** (M4, deployment L): Private immutable responses, FTS5/GIN search, 40-row keysets, 500-row guarded CSV export and permission checks.

- **F041 · File uploads and protected attachments** (M4, deployment L): Scoped expiring upload capabilities; private UTF-8 text/PDF/image files, bounded decoding/re-encoding, authenticated integrity-checked downloads and backup; no malware scanner.

- **F042 · Calculations and pricing logic** (M4, deployment L): Server-authoritative finite sum/product/choice-score calculation; exact currency and payment pricing remain M6.

- **F043 · Signatures and signed-document records** (M4, deployment L): Typed-name signature and acknowledgment with frozen statement and accepted response; no cryptographic signature/identity certification or generated signed PDF.

- **F044 · Surveys, polls and scoring** (M4, deployment L): Choice questionnaires, scores and boolean acknowledgment inputs; responses/export support local evaluation, no specialized statistical survey designer.

- **F045 · Save/resume, partial entries and abandonment** (M4, deployment L): Seven-day revisioned partial server drafts and explicit device recovery; no covert abandonment tracking or automatic abandoned-response marketing.

- **F046 · Offline capture and later synchronization** (M4, deployment L): Already-loaded form editing offline and original-payload retry across reload; no uncached offline site navigation or general distributed synchronization.

- **F047 · User registration and post creation** (M4, deployment L): Mailbox proof, private password step, owner approval and subscriber-only accounts; literal visitor contributions become moderated drafts.

- **F048 · Webhook and CRM/marketing handoff** (M4, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F049 · Payment fields / gateway checkout** (M4, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F050 · External CAPTCHA service** (M4, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F051 · Local users, roles and granular permissions** (M5, deployment L): Established fixed owner/editor/moderator/member roles, verified registration/approval, profile editing and authoritative resource policies. Custom role/capability authoring remains a refinement; no PHP-plugin permission compatibility claim.

- **F052 · Content and download access rules** (M5, deployment L): Reusable entitlement/group policies enforced on direct posts, media and learning APIs; anonymous search/feed/sitemap/relationship/collection exclusion and no-store/noindex protected responses. Explicit owner release; active course resources cannot be independently unprotected.

- **F053 · Membership state and entitlement administration** (M5, deployment L): Local entitlement assignment, start/expiry, revocation, disabled policies/accounts and administrative intervention. Payment-derived membership, renewals/refunds belong to M6.

- **F054 · Content drip and timed unlocks** (M5, deployment L): Absolute UTC opening, delay from valid entitlement/group join, sequential lesson prerequisites; same policy governs direct lesson downloads. Exact-boundary probes.

- **F055 · Course lessons and learner progress** (M5, deployment L): Draft/live versioned courses referencing shared published content, ordered up to 100 lessons, batched learner progress and deliberate republication. Native composition plus bounded CLI import/export; historical editions retained.

- **F056 · Quizzes, assignments, certificates and gradebook** (M5, deployment L): Authored multiple-choice quizzes (20 questions, 2–8 choices), 1–10 attempts, concurrent idempotency, text assignments, optimistic grading, feedback, resets and edition-specific local certificates. Revoked certificate links remain invalid after reissue; no public credential registry or arbitrary external grader.

- **F057 · Corporate subaccounts and gifting** (M5, deployment L): Delegated bounded seats for existing active accounts, composable group/entitlement authority and atomic one-use gifts. Corporate paid provisioning and checkout belong to M6.

- **F058 · Community profiles and discussion groups** (M5, deployment L): Private editable profiles and protected text discussions with owner moderation and escaped contributions. No public social-network directory or realtime chat claim.

- **F059 · Affiliate referral and commission records** (M5, deployment L): Fixed-target local referral visit counts and manually recorded/voidable commissions with unique references and minor-unit amounts. No unique-person attribution, purchase-derived commission or payout claim; commerce integration M6.

- **F060 · Enterprise identity / social-login provider** (M5, deployment E): Optional explicit HTTPS OIDC authorization-code/S256 PKCE/RS256 connector, issuer-subject binding, browser-bound one-use state and signed-claim checks; provider failures fail closed and local sign-in remains independent. Deterministic cryptographic/state checks and real HTTPS adapter exchange with TLS/PKCE/Basic credentials; actual vendor certification and SAML are not claimed.

- **F061 · Catalog, variants and digital products** (M6, deployment L): Published physical/digital/membership/booking products; up to 100 variants per product; finite stock and shared protected private downloads. No full WooCommerce extension ecosystem claim.

- **F062 · Cart, checkout state and orders** (M6, deployment L): Persistent authenticated 20-line carts, authoritative quote review, idempotent snapshotted orders, bounded private/merchant lists and separate settlement/fulfillment. Guest checkout remains outside this defined scope.

- **F063 · Inventory and reservation locks** (M6, deployment L): Transactional last-unit/last-slot holds, expiry/release, exact settlement allocations and final full-refund restocking; one owning process per site, distributed coordination remains M9.

- **F064 · Coupons, discounts and member pricing** (M6, deployment L): Integer basis-point coupons, product/member eligibility, bounded redemption counts, current entitlement pricing and M4 one-time reward codes. Recurring purchases are one fixed-price item without coupon/member adjustment.

- **F065 · Booking calendars, staff/resources and group capacity** (M6, deployment L): Explicit UTC resource/staff calendars, group capacity, overlap protection including reassignment, current staff authority and cursor access to later slots; external calendar sync remains F070.

- **F066 · Local appointment confirmations and reminder scheduling** (M6, deployment L): Durable service confirmation/reminder/cancellation through shared local mail_jobs; local spool or configured SMTP, no delivery claim when mail is disabled and no recall of in-flight mail.

- **F067 · Subscription schedules, proration and dunning state** (M6, deployment L): Paid period access, local renewal invoices/dunning/cancel, integer upgrade proration and accepted next-period downgrade prices; no simulated collection or hosted plan-change parity.

- **F068 · Refund/admin transaction workflow** (M6, deployment L): Actual payment receipts, fulfillment, reserved bounded partial/full refunds, own-grant revocation, manual affiliate payouts and fresh recovery of financial facts; records are not jurisdiction-certified tax invoices.

- **F069 · Card settlement and recurring collection** (M6, deployment E): Hosted Checkout with pinned API, raw signatures, canonical one-time/recurring exact-money reconciliation, delayed initial invoice identity, refund and period-end cancel. Local HTTPS protocol fixture is not an actual Stripe-account test; card settlement remains external.

- **F070 · Shipping rates/labels and calendar sync** (M6, deployment E): Explicit external boundary. Local merchant-set flat shipping/tax and UTC slots do not reproduce carrier labels/rates, external calendars, jurisdictional datasets or filing.

- **F071 · Current tax/compliance datasets** (M6, deployment I): Explicit external boundary. Local merchant-set flat shipping/tax and UTC slots do not reproduce carrier labels/rates, external calendars, jurisdictional datasets or filing.

- **F072 · Page caching, invalidation and preload** (M7, deployment L): Bounded anonymous publication/listing/sitemap cache, current-mutation invalidation, owner purge and preload of up to 20 recent unprotected pages without network fetches.

- **F073 · Browser-cache headers and compression settings** (M7, deployment L): Gzip toggle with per-request negotiation outside the uncompressed cache; asset-cache freshness 0–3600 seconds, default revalidation. Private/publication browser responses no-store.

- **F074 · Object-cache integration** (M7, deployment L): Bounded public serialized content projection cache; external shared object-cache integration remains M9.

- **F075 · Asset minification and loading controls** (M7, deployment L): Locked build-time minification and conditional feature bundles; configured template-scoped style/preload controls. No unsafe async CSS flash or arbitrary scripts.

- **F076 · Lazy loading, dimensions and preload hints** (M7, deployment L): New editor images record dimensions and lazy/eager priority; bounded authorized header-only inspection enriches composition/imported-local images transiently with checksum-keyed metadata reuse and graceful worker-pressure fallback. Studio image priority and canonical document priority share at most one rendered local preload. Historical publications stay unchanged; external images are never inspected.

- **F077 · Resize, compress and WebP/AVIF derivatives** (M7, deployment L): Native bounded WebP and AVIF derivatives, explicit size/format links, current original authorization on cache hits and separately bounded reusable derivative storage; animated formats preserve original delivery.

- **F078 · Native image binary / imgproxy worker** (M7, deployment W): Native in-process bounded image worker with separately admitted source reads/encoding, pixel/allocation budgets and AVIF/WebP reuse. No external imgproxy required.

- **F079 · Unused/critical CSS generation** (M7, deployment W): Native deterministic template-reachable stylesheet generation strips unrelated templates/components, preserves conditional/responsive rules and strict external-only style CSP. No heuristic above-fold pruning of arbitrary CSS.

- **F080 · Unused-image and database cleanup** (M7, deployment L): Owner preview/execute and CLI: conservative current/draft/historical cross-module UUID references, stale candidate rejection, bounded inventory, expired session removal, interruption-safe metadata-first unlink with retryable orphan preview. External links cannot be detected; no automatic historical/financial data deletion or online database compaction.

- **F081 · Geolocation / role-aware cache variants** (M7, deployment L): Optional server-validated session-role buckets and owner-provided IPv4/IPv6 CIDR region mappings. Shared public rendering supports site.role/site.region, current-role revalidation, separate region/role/query keys and conservative extra-cookie/private bypass. No geographic dataset or forwarded-proxy trust is inferred.

- **F082 · Video transcoding on own server** (M7, deployment W): Optional owner-installed FFmpeg/FFprobe worker, owner-only admin upload and CLI, fixed two-minute/1080p input profile and 720p MP4 output; private/public delivery, recovery and bounded local workers. Host OS codec sandbox/aggregate memory quotas remain operator prerequisites, not an application isolation guarantee. Progressive busy/error retry retains source; real browser decodes/plays MP4 at three widths. Current-session revocation wins before media publication.

- **F083 · Global image/video CDN and edge WAF** (M7, deployment D): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F084 · First-party pageviews, events and conversions** (M4, deployment L): Explicit-consent registered events, published public paths and server-confirmed response/offer conversions; GPC/DNT and withdrawal.

- **F085 · Forms/store funnels and custom dimensions** (M4, deployment L): Declared enum dimensions and pageview-to-response-to-offer funnel; arbitrary funnel builder and store purchase events are not claimed; commerce is M6.

- **F086 · Local dashboards and reports** (M4, deployment L): Bounded local event/path/category totals, funnel and recent geometry sessions; reports cover retained cookie sessions, not identity attribution.

- **F087 · Heatmaps and masked session replay** (M4, deployment W): Bounded geometry-only wireframe playback and click heatmap; never DOM text/field values/screenshots; not full DOM replay.

- **F088 · A/B experiments and variant allocation** (M4, deployment L): Stable consented-session A/B variants with retained impressions, sessions and claims; no statistical-significance guarantee.

- **F089 · Exit-intent, device and referrer popups** (M4, deployment L): Accessible targeted native dialogs, keyboard dismissal, device/referrer categories and public paths; exit-intent triggering is not claimed.

- **F090 · Scheduled/cookie/visitor-based targeting** (M4, deployment L): Start/end schedule, per-session impression cap, stable cookie allocation and renewed-purpose consent; not verified-person frequency.

- **F091 · On-site coupon wheels / offer campaigns** (M4, deployment L): Weighted local bounded-stock reward allocation and duplicate-safe session receipt; cookie reset is not fraud prevention; purchase/redemption is M6.

- **F092 · Google/ad-platform reporting datasets** (M4, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F093 · Cross-site audience / advertiser identity graph** (M4, deployment I): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F094 · SMTP settings and connection routing** (M4, deployment L): Typed local file outbox and up to four owner-configured SMTP routes with TLS/explicit loopback transport and redacted credentials.

- **F095 · Mail logs, resend, failure alerts and fallback** (M4, deployment L): Durable states/attempt history, bounded retries/fallback, visible uncertain outcomes and deliberate duplicate-aware resend; no third-party outage alert service.

- **F096 · Transactional templates and notification rules** (M4, deployment L): Canonical shared templates, fixed-recipient conditional notifications, confirmations and private registration proof.

- **F097 · Mailing lists, consent records and segmentation** (M4, deployment L): Purpose/version-specific pending/confirmed/withdrawn membership, typed segments, suppression, bounded export and audience deletion guards.

- **F098 · Newsletter design, queue and automation** (M4, deployment L): Shared rich composition, manual/scheduled bounded expansion and deduplicated confirmation-triggered templates; not arbitrary visual automation graphs.

- **F099 · Self-operated outbound mail server** (M4, deployment W): Owner-operated SMTP/MTA integration and disconnected .eml spool; no bundled internet-wide MTA/reputation infrastructure.

- **F100 · Chosen SMTP/transactional mail provider** (M4, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F101 · Gmail/Microsoft or remote CRM connectors** (M4, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F102 · Global inbox deliverability / shared reputation service** (M4, deployment I): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F103 · TOTP/passkeys and local login protections** (M7, deployment L): Local TOTP, one-use recovery codes and user-verified passkeys; replay/current-session/current-credential safeguards.

- **F104 · Security headers and hardening** (M7, deployment L): Startup-compiled typed HSTS/referrer/opener/public-resource controls, preserved strict same-origin CSP and inert previews, token-route no-referrer, nosniff and restricted browser capabilities on cache hits/errors. Owner-managed TLS; no arbitrary unsafe header text or automatic preload-list submission.

- **F105 · Local request rule engine and rate limits** (M7, deployment L): Real peer/path deny rules, bounded counters and global admission limits; no implicit forwarded trust.

- **F106 · File-integrity / known-pattern malware scans** (M7, deployment L): Paginated inventory/hash scans and explicit executable-pattern review warnings; current external signature intelligence excluded.

- **F107 · Audit logs and privileged action history** (M7, deployment L): Bounded privileged HTTP and host-owner CLI intent/outcome history, rotation-spanning inspection and last-200-record fresh recovery; shared bounded scheduler-cycle intent/outcome history records per-stage failure, duration/count and unresolved interruption; domain retry state remains authoritative.

- **F108 · Honeypots, proof-of-work and basic spam heuristics** (M7, deployment L): Opt-in local comment/form honeypot, destination-bound single-use proof-of-work and bounded link heuristic; no shared classification or human-identity guarantee.

- **F109 · Cookie banner and consent/script enforcement** (M7, deployment L): Local consent/GPC/DNT plus bounded checksum-pinned own-origin optional analytics scripts; full manifest grant binding, protected no-store delivery and withdrawal unload. Declared scripts share the analytics category; privileged owner code is not sandboxed and arbitrary third-party tag insertion is outside this safe local scope.

- **F110 · Consent records and data-request workflows** (M7, deployment L): Current local consent records plus password-proven account-linked JSON exports and durable owner-reviewed access/erasure requests with outcomes, pagination and fresh recovery. Anonymous/outside data needs independently verified handling; erasure is a documented owner action, never automatic financial deletion. Explicit account/profile removal revokes credentials, retains stable financial subjects and records partial completion; the last owner cannot be removed.

- **F111 · Cookie scan on own pages** (M7, deployment W): Optional locked Playwright same-origin sample scan, consent states and value-free cookie/storage/script metadata; finite scope.

- **F112 · Fresh vulnerability/reputation/signature datasets** (M7, deployment I): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F113 · Akismet-style shared spam classification** (M7, deployment I): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F114 · DDoS absorption and managed incident response** (M7, deployment D): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F115 · Consistent files + database snapshot** (M7, deployment L): Consistent current-schema logical database/file graph across SQLite/PostgreSQL.

- **F116 · Schedules, retention and pre-update backups** (M7, deployment L): Scheduled copies and per-destination retention/status; upgrade-prepare verifies a separately retained encrypted checkpoint and executable/archive identities before owner-managed replacement. Rollback is explicit fresh restore with the retained executable.

- **F117 · Encrypted backup packages and recovery keys** (M7, deployment L): Authenticated encrypted packages and separately held keys; tamper/wrong-key/fresh-recovery checks.

- **F118 · Incremental/deduplicated backup worker** (M7, deployment W): Native encrypted content-addressed chunks with independently complete manifests and verified preview/hash pruning.

- **F119 · Selective restore, clone and URL-aware migration** (M7, deployment L): Validated private extraction, dependency-aware editorial selection with exact preview package, URL-aware fresh clone with durable read-only hold and explicit stopped-host activation; shared financial/access/workflow graph retained, no live graph merge.

- **F120 · Portable restore utility independent of CMS** (M7, deployment L): Standalone same-binary archive inspect/extract without database/site installation; full restore remains empty-target.

- **F121 · Off-site copy to owner SFTP/NAS/secondary host** (M7, deployment D): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F122 · Point-in-time recovery and continuous replication** (M7, deployment W): PostgreSQL 17 engine archive/restore helpers with authenticated native WAL records and cluster/name binding; real verified base plus continuous WAL to named-point recovery with source removed. Native base protection/chain retention/engine tools and matching media are operator-managed; distributed read replicas remain M9.

- **F123 · Vendor vault / off-site disaster recovery service** (M7, deployment D): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F124 · One module registry and ownership resolver** (M9, deployment L): Validated effective module inventory, parent admission, unique domain ownership and named dependencies; no dynamic code loader/unloading claim.

- **F125 · Unified settings/schema export and import** (M9, deployment L): Current-version effective config export/private exact-plan import; native settings/models/schema travel in validated portable recovery graph. Redacted record is inspection-only; no cross-version settings migration claim.

- **F126 · Background jobs, retries and health history** (M9, deployment L): Bounded native cycle/stage history and domain-specific durable retry/uncertain outcomes; shared local admission and explicit crash pause. Not generic exactly-once network effects.

- **F127 · Reliable system-scheduled worker** (M9, deployment W): Independent supervised local-process worker and bounded --once system-scheduler cycle; external-worker server mode and graceful drain. Final termination/resource gates pending.

- **F128 · Local safe-mode and rollback controls** (M9, deployment L): Unresolved native mutation pauses all nodes; offline exact graph/external-effect review resumes; held fresh clones and encrypted pre-upgrade recovery support restore-based rollback. Full maintenance upgrade journey pending.

- **F129 · Authorized snippets and developer hooks** (M9, deployment L): Scoped external content integrations, proposed drafts, durable events and owner-hosted generation/layout examples; arbitrary in-process executable snippets are not sandboxed or enabled. Developer-hook scope reconciliation pending.

- **F130 · Owner-hosted fleet controller** (M9, deployment W): Independent read-only Rust CLI fleet observation across up to 32 delegated sites, isolated outages/revocation and finite secret-free transport. Bulk update/repair orchestration not included.

- **F131 · Uptime alerts while origin is down** (M9, deployment D): See the source catalog and recorded external/local boundary; no broader implementation claim.

- **F132 · Vendor license renewal / cloud quotas** (M9, deployment E): See the source catalog and recorded external/local boundary; no broader implementation claim.
