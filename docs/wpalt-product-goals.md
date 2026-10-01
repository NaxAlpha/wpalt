# wpalt — Product goals and requirements

Status: Agreed-scope draft with evidence, milestone PR and migration policies, version 0.3  
Date: 2026-09-30  
Companions: [Development methodology](wpalt-development-methodology.md) · [Milestones](wpalt-milestones.md)

## 1. End goal

Build **wpalt**, a complete, self-contained Rust application that replaces WordPress and incorporates the locally implementable capabilities of the popular plugins studied in our research. It must provide a robust, user-friendly administration experience, a powerful theme composition system, and tightly integrated features with substantially lower memory usage and disk footprint than comparable WordPress installations.

The product must operate on owner-controlled infrastructure: a local machine, a single server, or a complex multi-server installation. Installed functionality must work without a mandatory wpalt vendor account or runtime license check. External services remain optional integrations where their capabilities require outside infrastructure or data.

This is a complete-product objective, delivered in stages. An initial milestone must not be presented as full WordPress or plugin feature parity.

Each milestone must deliver a significant, usable end-user system that moves toward this goal. Repository setup, refactoring, query tuning and other internal improvements are work within milestones, not standalone milestone outcomes. Each milestone has verifiable user journeys, a runnable release and recorded evidence.

Before starting a milestone, we may refactor or optimize previously delivered capabilities to make the next stage easier. Within each milestone, first make each functional step work correctly, then verify and optimize that step. After integrating its steps, review and optimize the whole milestone for security, performance, reliability, resource usage and user experience. Baseline security and data correctness apply from the first working implementation.

## 2. Repository and project ownership

- The intended repository is **`wpalt`**, public, under the user's GitHub account.
- Once repository creation and development begin, clone it into this workspace and perform project work inside that checkout.
- Move these requirements, the development methodology and the milestone roadmap into the repository's `docs/` directory when development starts. Commit them before application implementation and use the repository copies as the maintained source of truth.
- Carry the supporting research/catalog into an appropriate repository location and update relative links. Commit the evolving feature-parity matrix so the agreed scope survives long development sessions.
- Create a pull request for every milestone, containing its substantial end-user delivery, verification evidence, documentation and applicable migration/change notes.
- Development was authorized after the planning documents were agreed. The public repository now exists and M1 is delivered for review; these repository documents are canonical. Licensing remains undecided before public adoption.

## 3. Product principles

### Owner-controlled operation

Administration, content, themes, media, local analytics, local security controls, and backup/restore must be managed on the owner's infrastructure. Restoring or moving a site must not require a wpalt vendor login. The owner controls storage, configuration, credentials and exports.

Running locally does not prohibit normal local administrator accounts. It also does not mean payment processing, independent disaster-recovery storage, global CDN delivery, or proprietary external datasets can be reproduced entirely on the origin server.

### Integrated functionality

Use shared content definitions, permissions, conditions, events, jobs, storage and configuration across features. For example, a content field should be usable in the editor, a form, a theme binding and structured output without maintaining unrelated definitions. Avoid duplicate SEO output, overlapping cache owners, contradictory access rules and separate settings interfaces for closely related work.

### Modern, useful design

Preserve useful capabilities and migration paths while improving workflows and architecture. Modern web and application frameworks may be used where they improve usability, robustness or development efficiency. Rust is the application backend; the frontend approach remains an architecture decision. Feature parity does not require copying every legacy interaction or internal implementation.

### Measured efficiency

Low memory use, low disk footprint, efficient queries and fast requests are explicit requirements. Measure these against equivalent workloads and feature sets. Disabling optional modules should remove their unnecessary background work, hooks and frontend assets. Do not claim an advantage from comparing a minimal wpalt installation to a much more capable reference site.

## 4. WordPress capability coverage

The parity matrix must enumerate and track the relevant WordPress core behavior, including:

- Installation, initial setup, configuration, updates and operational health.
- Content types, fields, relationships, posts, pages, taxonomies and reusable content.
- Drafts, revisions, autosave, preview, scheduling and publishing workflows.
- Users, roles, permissions, authentication and administrative access.
- Media upload, organization, metadata and delivery.
- URLs, routing, navigation, feeds, search and public rendering.
- Comments, moderation and associated permissions where included in core parity.
- Theme management, templates, reusable compositions and content bindings.
- APIs, extensibility, import/export and migration.

This list is the starting baseline. Inspection of WordPress and its documented behavior will expand it into explicit, reviewable requirements. A feature remains incomplete until its relevant permissions, failures, migration behavior and operational requirements are addressed.

## 5. Admin panel

Provide a coherent, accessible administration interface for content, users, media, themes, modules, settings, backups and operational information. It must support ordinary site owners as well as experienced developers.

- Reuse editing and configuration patterns across modules.
- Make permissions, validation failures, background job status and recovery actions understandable.
- Provide clear previews and safe publishing workflows.
- Present useful diagnostics without requiring users to understand internal implementation details.
- Keep CLI and configuration-file capabilities aligned with the admin panel where they represent the same operation or setting.

## 6. Themes and composition

Provide a theme system and a composition system capable of building complex websites beyond the current WordPress model. The initial design space includes reusable components, layouts, templates, design tokens, dynamic content bindings, conditions and responsive presentation.

Theme authors should be able to assemble complex experiences without duplicating data models or abandoning the shared permissions and rendering system. Site owners should be able to edit and preview supported compositions through a usable admin experience.

The exact composition language, theme packaging, authoring tools and rendering boundaries must be specified and tested during architecture work. Theme execution and extension permissions require explicit security boundaries. Existing WordPress PHP themes and plugins are not assumed to run unchanged; their runtime compatibility would be a separate scope decision.

## 7. Plugin capability clusters

The companion research catalog contains 40 plugin profiles and 132 capability groups. It is a discovery baseline, not a claim that those capabilities are already implemented. Preserve traceability from each selected capability to its requirements and tests.

| Cluster | Intended capability area |
|---|---|
| Content model and editorial | Typed fields, relations, repeaters, revisions, options and editorial permissions. |
| Design and presentation | Composable layouts, components, patterns, templates, navigation, dynamic bindings and popups. |
| SEO and discovery | Metadata, structured data, sitemaps, redirects, link tools, multilingual URLs and local business data. |
| Forms and workflow | Form fields, submissions, uploads, conditions, calculations and queued actions. |
| Identity, access and learning | Registration, roles, restricted content, memberships, courses, progress and quizzes. |
| Commerce and reservations | Catalogs, orders, inventory, discounts, subscriptions, bookings and capacity management. |
| Media and performance | Image processing, caching, asset handling, lazy loading and cleanup. |
| Analytics and conversion | Owner-hosted events, reports, funnels, campaigns, experiments and applicable interaction analytics. |
| Email, CRM and audience | Mail queues, logs, retries, lists, segments, templates, opt-ins and contact fields. |
| Security, consent and abuse | Authentication protections, rate limits, headers, audit trails, consent controls and local abuse rules. |
| Backups, recovery and migration | Consistent snapshots, scheduling, retention, encryption, restore, import/export and site moves. |
| Development and operations | Module management, configuration, jobs, diagnostics, health and applicable fleet workflows. |

The full locally implementable scope remains the end goal. Commerce, learning and advanced composition may require substantial later stages; staging must not silently remove them from the goal.

Capabilities based on payment networks, external mail delivery, remote monitoring, distributed edge infrastructure or proprietary intelligence must be classified separately. Their connectors must not become hidden prerequisites for unrelated local functionality. Exact parity with an external service's proprietary data or global infrastructure is not implied.

## 8. Deployment and configuration

Support three explicit deployment modes:

1. **Local:** straightforward installation for development, evaluation and owner-operated local use.
2. **Single server:** application, database, media and jobs can run on one host, with optional independently located backup copies.
3. **Multi-server:** application instances coordinate through supported database, storage and job arrangements, with documented consistency and failure behavior.

Provide extensive CLI and configuration-file control, with typed validation, documented precedence, actionable errors and configuration inspection that redacts secrets. Document which settings require restart and which can change safely at runtime. Define support by a validated configuration matrix rather than promising every arbitrary combination.

The self-contained distribution goal means a cohesive application and installation path with bundled application assets. Optional worker tools and external infrastructure must be declared explicitly. It does not promise that PostgreSQL, distributed storage or every optional processor exists inside a single executable.

## 9. Database and request performance

Support **SQLite and PostgreSQL** as first-class choices for their documented deployment modes. Provide database-specific migrations, indexes, queries and transaction behavior when their semantics differ. SQLite is not assumed to be a shared writable database across independent application servers.

Design every query and request path deliberately. Validate critical paths using realistic datasets, query plans, query counts and benchmarks. Address pagination, N+1 access, over-fetching, bounded work, transaction contention, cache invalidation and backpressure. No claim is made that every query has a universally optimal plan for every possible workload.

## 10. Diagnostics and security

Provide a detailed debug mode covering request handling, database operations, jobs, timing, errors and security events. Correlate related events so developers can follow an operation through the system.

“Log everything” means comprehensive diagnostic visibility, with passwords, tokens, keys and sensitive payloads redacted. Debug logging must have documented overhead, retention and access controls. Ordinary production operation must remain observable without requiring unrestricted debug output.

Security is part of feature completeness: authentication, authorization, session handling, validation, upload safety, template execution, secret handling, dependency risks and administrative operations must have relevant verification. Multi-server behavior must explicitly address concurrency, races, duplicate work and partial failure.

## 11. Backups and recovery

Provide locally managed backup creation, scheduling, retention, integrity checking and restore. Database and files must form a usable recovery point. Support owner-chosen independent destinations without requiring a vendor backup service.

A same-server backup protects against some application mistakes but cannot recover from losing that server. Document and support the independent-copy workflow. Recovery must work on a fresh installation, including configuration and media, with independently available recovery keys where encryption is used.

## 12. Acceptance and open decisions

The user will review understandable high-level acceptance tests of the final application. Component and cluster tests, performance evidence, security verification and failure/recovery exercises support that review.

The [milestone roadmap](wpalt-milestones.md) defines cumulative end-user outcomes and verification for intermediate releases. Preserve earlier user capabilities and their relevant acceptance guarantees, updating journeys when the design deliberately changes. This is not a promise to preserve earlier APIs, schemas or configuration formats. Its completion record must distinguish functionality that works from functionality that has also passed the milestone's optimization and hardening checks.

If scope or product direction changes, record the proposed change, rationale, consequences, affected milestones and decision. Realign with the user on material changes to the agreed goal; document routine implementation decisions in architecture records. Update these goals, the methodology, roadmap and parity matrix together where affected. A reordering or internal refactor does not silently remove a requirement.

Before making numerical efficiency claims, establish representative workloads and agree on measurable budgets. Frontend framework, theme composition format, extension model, precise deployment matrix, release strategy and performance targets remain open architecture decisions. Document these decisions without weakening the agreed end goal.

## 13. Current guidance and future updates

Ground externally governed behavior in current authoritative information when designing, implementing and verifying it. SEO is one example; the same protocol applies to security, accessibility, privacy/consent, structured data, browser behavior, database capabilities, APIs, payment integrations and other features where outside guidance affects correctness. Purely internal features still record their contract and verification basis.

Maintain a versioned evidence record for each applicable feature: authoritative sources, applicable versions, date checked, relevant requirements/recommendations, implementation mappings, tests, review triggers and next review date. Distinguish normative requirements, provider guidance and engineering inference. A research snapshot or a plugin's behavior alone is not proof of current correctness.

Provide a development/release mechanism to review changing guidance, assess impact, update implementation and tests, and record the resulting evidence version. For SEO, check relevant search-engine documentation and structured-data specifications rather than assuming a plugin's current defaults are authoritative. Do not promise rankings or automated correctness merely because a rule is current.

Keep rules and validation fixtures versioned with releases. Where independent data/rule updates are appropriate, provide validated, versioned update bundles with review and rollback. Installed features must keep working offline using their bundled supported rules; mandatory live guideline checks or vendor accounts must not become runtime dependencies.

## 14. Breaking changes and migration lifecycle

Treat M1–M8 as pre-adoption development releases: the user does not expect public production use before M9. Favor clear current implementations over layers of backward compatibility between milestones. Breaking internal APIs, schemas, configuration or theme formats is acceptable through documented decisions. For owned reference/development data, provide an appropriate migration or an explicitly documented reset when the data is disposable; never silently discard meaningful data.

Beginning with M9 and future adopted releases, use explicit supported upgrade paths and tested, versioned migrations for persisted data, configuration and affected themes/integrations. When breaking changes are needed, prefer a bounded migration to the new model rather than permanently carrying old runtime behavior.

Any temporary compatibility layer must have a reason, supported version range, replacement/migration path and removal release or review deadline. Remove it as soon as the documented migration and support conditions permit. Retain necessary migration capability separately from legacy runtime code; removing a compatibility layer must not strand users on a supported upgrade path. Multi-server upgrades must explicitly select a bounded mixed-version window or a controlled maintenance migration.

Reference baseline: [Local-first plugin and CMS research](research/local-first-plugin-and-cms-research.html) and [structured capability catalog](research/local-first-plugin-and-cms-data.json). The earlier research's recommendation to build a WordPress plugin first is superseded by the agreed standalone Rust application direction.

## 13. Frontend foundations — agreed 2026-10-01

A supporting PR before M3 establishes a precise design system, component/placement measurements, visual regression evidence and explicit aesthetic/UX review. Apply it to improve current administration and future frontend work. Testing must verify useful UI guarantees and remain understandable and bounded. Theme/customization capabilities remain assigned to existing milestones unless a material new product direction is explicitly agreed.
