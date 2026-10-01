# wpalt — Milestone roadmap

Status: Milestone plan with evidence, PR and migration policies, version 0.2  
Date: 2026-09-30  
Companions: [Product goals](wpalt-product-goals.md) · [Development methodology](wpalt-development-methodology.md)

## Delivery rules

Every milestone delivers a substantial end-user system, building on the previous release. The roadmap describes proposed delivery boundaries, not completed functionality or a fixed schedule. M1 and M2 are merged in PRs #1 and #3. Frontend foundation preparation is active before M3; M3–M9 are planned.

Each milestone follows the same process: prepare and, where useful, refactor the preceding system; define the milestone contract; make each functional step work; verify and optimize each step; integrate and optimize the complete system; deliver a runnable release with evidence. Working correctly includes baseline security and data integrity from the outset.

Preparation, infrastructure work, benchmarks and refactoring are necessary work inside a milestone. They are not substitute end-user deliverables. Preserve earlier user capabilities and update their acceptance journeys when a deliberate redesign changes the workflow; this does not require preserving obsolete internal interfaces. Scope changes require a documented decision and realignment where they materially affect the goal.

Create a primary delivery pull request for every milestone and record its URL and verification/review/merge status. Move these planning documents into the repository before implementation begins. Each milestone includes a current authoritative-guidance review for its affected features and records the evidence version; SEO is one example of a protocol that applies throughout the system.

M1–M8 are pre-adoption development stages; public repository visibility does not imply production support. Avoid backward-compatibility layers between them. Breaking changes need clear notes and an appropriate migration or explicit reset of disposable development data. M9 establishes the supported public release and upgrade baseline. Future breaking changes use tested migration paths and bounded compatibility bridges that are removed promptly when their documented conditions permit.

The full parity matrix remains authoritative for capability coverage. The examples below establish demonstrable outcomes, not an exhaustive feature checklist. Before implementation of each milestone, assign its remaining detailed capabilities and specify the exact completion contract. Unassigned requirements remain visible; they must not disappear between stages.

## Roadmap overview

| Milestone | Cumulative system delivered | Main user outcome |
|---|---|---|
| M1 | Usable publishing CMS | Install, author and publish a real content website. |
| M2 | Composable website builder | Build complex, reusable, data-driven themes through a coherent authoring experience. |
| M3 | Multilingual discovery platform | Run a multilingual, searchable site with integrated SEO and discovery controls. |
| M4 | Business and audience platform | Collect leads, manage contacts, communicate and measure conversion locally. |
| M5 | Membership and learning platform | Operate restricted communities and structured learning experiences. |
| M6 | Commerce and reservations platform | Sell products and memberships and manage bookings without fragmented business state. |
| M7 | Resilient owner-operated platform | Operate, protect, optimize, back up and recover a substantial site independently. |
| M8 | Migratable and extensible platform | Move supported WordPress sites into wpalt and build integrations without losing data control. |
| M9 | Distributed full-platform release | Run the cumulative system across supported multi-server configurations and reconcile final parity. |

Local and single-server operation remain supported throughout. SQLite and PostgreSQL are established in M1 and tested through later relevant features. Multi-server execution is introduced and verified deliberately in M9; foundational choices must account for it earlier.

## M1 — Usable publishing CMS

**Delivered system:** A site owner can install wpalt, administer it and publish a functioning blog or content website on their own infrastructure.

**Functional steps:**

1. Install and configure using CLI/config files; initialize SQLite or PostgreSQL; create the administrator and access a usable admin panel.
2. Manage pages, posts, initial structured content, categories/tags, drafts, autosave, revisions, preview and scheduled publishing.
3. Upload and manage media; compose basic pages using an initial theme/component model; manage navigation.
4. Serve public pages, search, feeds and moderated comments; provide initial export and manual backup/restore workflows.

**Verifiable output:** A distributable application, a populated example site and readable journeys that install, author, preview, publish, search, moderate, restart and restore the site. Run relevant journeys against both databases. Confirm unauthorized publication and protected media access fail appropriately. Scheduled publication must survive ordinary restarts according to its documented behavior.

**Whole-system optimization:** Measure public rendering, authoring interactions, representative query plans, query counts, idle/active memory and installation footprint. Verify session and upload boundaries, concurrent edits, secret-redacted debug logs and usable failure messages. Record measured baselines rather than claiming the full efficiency goal is already reached.

Repository creation, moving and committing the planning documents, architecture selection, reference environments and test infrastructure are preparation within M1. Establish the feature-evidence manifest, freshness checks and migration/compatibility register in this preparation. Subsequent milestones may redesign initial models; meaningful data needs preservation or an explicitly documented decision, while disposable reference data may be reset.

## M2 — Composable website builder

**Delivered system:** A designer or site owner can create a complex, responsive website with reusable themes and dynamic content without maintaining disconnected plugin layouts.

**Functional steps:**

1. Expand structured content to reusable typed fields, relations, repeaters and shared site options.
2. Provide theme packaging, reusable components, layouts, design tokens and composition rules.
3. Bind content to templates, lists and conditional presentation; support responsive editing and previews.
4. Manage and switch themes, revise compositions and publish changes safely through the admin panel.

**Verifiable output:** A demonstrable multi-template website using repeaters, relationships, reusable components and dynamic listings. Review journeys that change a shared component, preview its effect, publish, revert and switch themes without losing content. Include invalid bindings, unauthorized edits and unsupported theme inputs.

**Whole-system optimization:** Evaluate editor responsiveness, rendered asset cost, composition/render latency, query expansion and cache invalidation. Test cyclic/deep compositions, excessive work and theme execution boundaries. Validate that public output and the preview agree and remain usable across representative screen sizes.

This milestone must deliver capabilities beyond the basic theme model, not merely rename WordPress blocks. Record the concrete gains and remaining limitations.

## M3 — Multilingual discovery platform

**Delivered system:** A publisher can operate a multilingual site with integrated search, SEO, redirects and discovery settings.

**Functional steps:**

1. Manage translated content and shared relationships; configure language routing and editorial workflows.
2. Generate metadata, canonical/hreflang output, structured data and sitemaps from shared content definitions.
3. Manage redirects, navigation, local business information and relevant link checks.
4. Provide useful search and discovery administration, with optional external search-performance data clearly separated.

**Verifiable output:** A multilingual site with demonstrable language navigation, search, redirects, correct publication-dependent sitemaps and configurable SEO output. Check representative rendered output and compare supported reference behavior. Test missing translations, conflicting URLs, redirect loops and drafts excluded from discovery.

**Whole-system optimization:** Inspect search and sitemap queries at meaningful content volumes, prevent unbounded crawling work, verify cache invalidation across languages and enforce access controls in search results. Review the admin experience for avoiding duplicate metadata and conflicting settings. Verify SEO and structured output against current applicable primary guidance, recording dates/versions, mapped tests and future review triggers. External provider failure must not disable local discovery features.

## M4 — Business and audience platform

**Delivered system:** A business can publish landing pages, capture and manage leads, communicate with its audience and measure engagement using owner-controlled data.

**Functional steps:**

1. Build advanced forms with shared field definitions, conditions, calculations, uploads and applicable submission workflows.
2. Manage entries and contacts, audience lists, segments and consent through one administration experience.
3. Compose campaigns and messages; queue, log, retry and inspect delivery through configured mail transport or optional connectors.
4. Add consent-aware local analytics, funnels, conversion events, popups and experiments; extend interaction analytics according to the parity contract.

**Verifiable output:** A complete journey from visitor consent and form submission to a stored contact, queued communication and a local conversion report. Include conditional forms, duplicate submissions, opt-out, export/deletion and provider outage behavior. Demonstrate that local records and reports remain usable without a vendor account.

**Whole-system optimization:** Measure submission latency, report queries, event-storage growth and campaign job throughput. Test retry/idempotency boundaries, unauthorized contact access, abuse/resource limits, privacy controls and disabled-module costs. Mail delivery infrastructure is configurable; internet-wide deliverability is not guaranteed by local queue management.

## M5 — Membership and learning platform

**Delivered system:** An operator can run a private community or learning site with roles, entitlements, restricted content and structured courses.

**Functional steps:**

1. Provide registration, member profiles, granular permissions and entitlement administration.
2. Define protected content, access rules, groups and drip schedules using shared policy primitives.
3. Create courses, lessons, quizzes and progress workflows through the admin and theme systems.
4. Provide member-facing experiences and operational reports; prepare paid-entitlement integration for M6.

**Verifiable output:** A course/community site where different members receive the right content, complete learning activities and retain progress after restart. Test access via public pages, APIs, media and jobs. Exercise access expiry, role changes, concurrent progress updates and administrator interventions. M5 can assign entitlements locally; paid purchase flows are completed in M6.

**Whole-system optimization:** Profile permission checks and learner dashboards, verify protected-content caching and revocation behavior, test schedule boundaries and accessible learning interactions. Shared permissions must govern every delivery surface consistently.

## M6 — Commerce and reservations platform

**Delivered system:** A merchant can operate a store and booking service integrated with content, contacts, memberships and learning.

**Functional steps:**

1. Manage products, variants, stock, prices, discounts and the applicable shipping/tax configuration specified in the milestone contract.
2. Provide carts, checkout, orders, invoices/records and fulfillment workflows.
3. Integrate configured payment providers and recurring-payment state; connect purchases to member/course entitlements.
4. Manage reservations, calendars, resources, availability and capacity, with relevant cancellations and refunds.

**Verifiable output:** Review complete product-purchase, paid-membership and booking journeys using deterministic payment test adapters and available provider sandbox verification. Include failed/repeated callbacks, refunds, abandoned checkouts and simultaneous last-unit/last-slot purchases. Confirm no overselling or overbooking under the defined concurrency tests.

**Whole-system optimization:** Measure checkout/order queries and lock contention, verify financial precision and transactional invariants, enforce idempotency and protect sensitive records. Review storefront and operator workflows together. State supported business rules explicitly; neither universal tax compliance nor compatibility with every payment provider is implied.

## M7 — Resilient owner-operated platform

**Delivered system:** A site operator can maintain, secure, optimize and recover the cumulative application through integrated controls rather than assembling unrelated operations plugins.

**Functional steps:**

1. Expand manual recovery from M1 into scheduled, consistent backups with retention, encryption, integrity checks and owner-selected independent destinations.
2. Provide fresh-install recovery, safe site moves, upgrade recovery and interrupted-job handling.
3. Integrate advanced media processing, caching, asset optimization, resource controls and applicable cleanup workflows.
4. Expand local authentication protections, security/consent administration, audit trails, diagnostics and operational health.

**Verifiable output:** A substantial sample site is backed up, its original instance is made unavailable and it is recovered on a fresh instance without a vendor account. Demonstrate corrupt/incomplete backup handling, encrypted recovery, interrupted work and independent-copy failure reporting. Review relevant security controls and performance settings through the admin panel and CLI.

**Whole-system optimization:** Compare equivalent enabled capabilities against a recorded WordPress/plugin installation for memory, disk, request latency and operational cost. Exercise storage exhaustion, worker limits, failed upgrades, cache correctness and permission boundaries. Validate the recovery runbook and user-visible failure explanations. Fundamental security and backups already exist before M7; this milestone makes operation substantially more complete.

## M8 — Migratable and extensible platform

**Delivered system:** An owner of a supported existing site can migrate into wpalt, retain useful data and extend the site through documented APIs and extension boundaries.

**Functional steps:**

1. Provide WordPress migration assessment, supported core imports, media mapping, redirects and explicit unsupported-item reports.
2. Add prioritized plugin-data migration adapters across implemented clusters, with preview, validation and rollback paths.
3. Provide documented APIs, webhooks and a controlled extension model integrated with permissions, jobs, themes and configuration.
4. Support portable exports and independently authored themes/integrations using documented contracts.

**Verifiable output:** Migrate recorded reference sites representing content and selected business workflows; reconcile counts, relationships, URLs, media, access rules and supported operational records. Demonstrate warnings for unsupported mappings rather than silent loss. Build and exercise a sample independently authored integration and theme; export and recover the migrated data.

**Whole-system optimization:** Test bounded import resource usage, restart/retry behavior, large datasets, untrusted archives and extension permissions. Review migration UX, mapping explanations and rollback. Existing PHP plugins/themes are not executed unchanged, and the supported adapter list is explicit. Earlier modules expose the APIs they need; M8 delivers a coherent external development and migration experience.

## M9 — Distributed full-platform release

**Delivered system:** An operator can deploy and run the cumulative wpalt system in supported multi-server configurations while retaining local and single-server choices.

**Functional steps:**

1. Provide reproducible deployment/configuration for multiple application instances, PostgreSQL, suitable shared media and coordinated workers.
2. Establish consistent sessions, job ownership, cache invalidation and side-effect handling across instances.
3. Deliver scaling, rolling-upgrade and recovery workflows with understandable health and operational controls; include applicable owner-hosted fleet capabilities from the parity matrix.
4. Reconcile remaining WordPress core and selected locally implementable plugin capabilities, completing gaps or explicitly realigning scope before final completion.

**Verifiable output:** Run cumulative acceptance journeys across multiple instances; interrupt a worker/application instance and verify the documented recovery and consistency guarantees. Exercise concurrent publishing, submissions, stock/booking updates and access changes across nodes. Demonstrate supported upgrades, backup recovery and configuration exports. Establish the public version/support baseline and the versioned migration lifecycle, including compatibility-removal checks and the distributed upgrade strategy. Re-run representative local SQLite and single-server PostgreSQL journeys to protect simpler deployment modes.

**Whole-system optimization:** Measure scaling limits, contention, tail latency, footprint and failure recovery under recorded workloads. Verify distributed permission/cache behavior, duplicate-work prevention and provider failure isolation. Confirm debug evidence is useful and redacted across nodes. Review final UX and operator runbooks alongside the complete parity matrix.

SQLite remains supported for its validated local/single-server modes; it is not used as an arbitrarily shared writable file across independent servers. This release is complete only when the agreed capability matrix and final acceptance criteria are satisfied, not merely because the multi-server demo passes.

## Completion evidence required for every milestone

Each release must include:

- A versioned runnable artifact or documented reproducible build, configuration examples and a populated demonstration suitable for review.
- A concise statement of the new end-user system, supported configurations and known limitations.
- Readable high-level acceptance journeys with expected outcomes and actual verification results.
- Relevant component/cluster evidence, failure cases and earlier-journey regression results; avoid repetitive test inflation.
- Measured performance/resource results and security/concurrency/UX review findings appropriate to the milestone, with conditions and unresolved issues recorded.
- Updated parity matrix, decision records, operation instructions and milestone status.
- The milestone delivery PR with linked evidence and explicit verification/review/merge status.
- Affected feature-guidance records checked for current applicability, with update decisions and future review dates.
- Breaking-change notes, relevant migration/reset evidence and any compatibility-layer removal or expiry decisions.

Before work starts, define milestone-specific measurable criteria, including dataset sizes, load conditions and acceptable performance/resource budgets. Do not invent universal numbers without measurements. A milestone is not complete if its agreed criteria remain unmet; record the gap and make an explicit decision about resolving it or changing the contract.

Final review is a release gate across the cumulative system, not an internal-only extra milestone. It includes the user's review of understandable acceptance tests, full capability reconciliation, migration limits, security findings and substantiated efficiency claims.

## Change and progress record

Maintain each milestone's status as planned, active, functional, hardening or complete. Track functional steps separately so working functionality is not confused with finished optimization. A completed milestone has the required delivery PR and verification evidence; PR review and merge remain separately recorded states. Record releases, PRs and evidence as they are produced; see the active milestone contract and verification record for current status.

For a proposed change, record the affected requirement/milestone, reason, options, consequences and resulting decision. Realign material scope or direction changes with the user. Routine refactors and implementation choices within the agreed goal can proceed with documented reasoning. Update all affected documents after the decision.

Milestone boundaries may be refined or reordered when dependencies are understood. They must continue to deliver substantial end-user outcomes and remain traceable to the final goal.

### M1 delivery record — 2026-09-30

M1 is complete as a reviewable delivery: functional publishing steps and hardening are verified. [Primary delivery PR #1](https://github.com/NaxAlpha/wpalt/pull/1) is open; current-head CI must be green before marking the PR ready. Independent Linux application and Rust 1.85 compiler-floor jobs have passed; their current results remain visible on the PR. See [M1 contract](m1-contract.md) and [verification evidence](evidence/m1-verification.md). User review and merge are pending. M2–M9 remain planned; no material product-scope change was needed. Architecture, deployment limits and the compiler dependency correction are documented in ADR 0001.

### M1 merge and M2 preparation — 2026-10-01

The user authorized merging M1 and proceeding to M2, with automatic clean post-merge builds first. PR #1 merged as `674651d73e178a4f8aca20ab17488059211275f2`. Build-pipeline preparation is documented in operations.md; it supports the milestone delivery process rather than becoming a separate product milestone. M2 starts after its merged-source build is verified.

### M2 active — 2026-10-01

[Build preparation PR #2](https://github.com/NaxAlpha/wpalt/pull/2) merged as `0cce274912ada0e92d51be3e2874088916dbea05`. Its [actual main run](https://github.com/NaxAlpha/wpalt/actions/runs/36778736692) passed all three jobs; the clean archive was downloaded and its source identity/checksums/permissions verified. M2 is active under [its contract](m2-contract.md). M3–M9 remain planned. [M2 delivery PR #3](https://github.com/NaxAlpha/wpalt/pull/3) is open. Its initial implementation passed all three GitHub jobs; final refinements and evidence require current-head verification before ready-for-review status.

### M2 verified local delivery — 2026-10-01

The composable studio, typed authoring, published-data bindings, theme/options publication/history, portable packages and schema-2 upgrade/recovery are implemented. Twelve readable acceptance journeys pass on SQLite and PostgreSQL 17; native release browser/CLI checks pass. Performance, SQL plans, security/data boundaries and remaining capability limits are recorded in [M2 evidence](evidence/m2-verification.md). Final-head CI and user review/merge remain distinct delivery states. M3–M9 remain planned.

### M2 merged; frontend preparation before M3 — 2026-10-01

[PR #3](https://github.com/NaxAlpha/wpalt/pull/3) merged as `e527cd118b1a9326e3d8288e506746a1908afb52`; [actual main build](https://github.com/NaxAlpha/wpalt/actions/runs/36793117434) passed and its clean archive was downloaded/verified. The user requested a dedicated supporting frontend PR before M3. Its [contract](frontend-foundations.md) establishes tokens, shared controls, precise browser measurements, visual baselines and human aesthetic review, with improvements to current administration. It does not create a small internal milestone or replace M3's substantial user outcome. M3–M9 remain planned; theme evolution follows existing M2/M8 scope.

[Frontend supporting PR #4](https://github.com/NaxAlpha/wpalt/pull/4) contains its design/measurement foundation, current admin improvements and review evidence. Readiness requires ordinary platform comparison and all current-head application/compiler/clean-build jobs; review/merge remain separate. M3 has not started.

### Calm default UI redesign before M3 — 2026-10-01

Frontend foundation [PR #4](https://github.com/NaxAlpha/wpalt/pull/4) merged as `c0b2ff65cc0e952da0b3fa63109f38ca801f3ef1`; actual-main checks and downloaded clean-build hashes were verified. The user then authorized a separate current-interface redesign: [calm cardless administration contract](calm-admin-design.md). This supporting PR applies the established measurement foundation to an open, coherent default admin experience; future milestone features and public theme evolution remain in their existing scope. M3 has not started.
