# wpalt — Development methodology

Status: Agreed-scope draft with evidence, milestone PR and migration policies, version 0.3  
Date: 2026-09-30  
Companions: [Product goals and requirements](wpalt-product-goals.md) · [Milestones](wpalt-milestones.md)

## 1. Purpose

Develop wpalt toward the complete product described in the product goals, with explicit feature tracking, useful tests and measured engineering claims. Preserve the user's requirements throughout implementation rather than relying on conversational memory.

This document defines the working process. It does not report completed implementation, successful benchmarks or verified feature parity.

## 2. Durable project records

Once work starts in the public `wpalt` repository, commit and maintain:

- Product goals and this methodology as the source of truth for scope and process.
- A feature-parity matrix connecting WordPress core and researched plugin capabilities to requirements, implementation and verification.
- Architecture decision records for choices affecting data, configuration, themes, extensions, deployment and security.
- The milestone roadmap and a progress record showing completed work, remaining work and known limitations.
- Configuration, operation, backup/restore and contributor documentation.
- A readable high-level acceptance suite and instructions for running it.
- Benchmark scenarios, measurement conditions and reproducible results.
- Versioned feature-guidance/evidence records, their review schedule and update decisions.
- A migration/support matrix and register of any temporary compatibility layers, including removal criteria.

Move all three current planning documents into `docs/` and commit them before application implementation. Move supporting research/catalog files into an appropriate repository location and repair their links. The repository documents become canonical; avoid maintaining divergent planning copies here and in the checkout.

Update records when behavior or scope changes. Distinguish user-approved scope changes from implementation sequencing. A stage may defer a capability; it must not mark that capability complete or quietly discard it.

For changes to scope or direction, write a decision record describing the proposal, rationale, alternatives, effect on the final goal and impact on existing milestones. Realign with the user before implementing material product-scope changes. Routine technical decisions within the agreed scope can proceed with documented reasoning. Keep the goals, methodology, roadmap and parity matrix consistent with the resulting decision.

## 3. Reference-system investigation

The user authorizes cloning WordPress source, running it in containers and installing plugins' free versions in isolated development environments to understand behavior and compare implementations.

- Use disposable reference installations and synthetic data.
- Record versions, enabled features, configuration and the behavior being examined.
- Study both implementation and user workflows where useful: publishing, forms, theme composition, permissions, media, recovery and configuration.
- Distinguish behavior directly observed in a free version from paid behavior described in vendor documentation.
- Do not infer complete premium parity from a free plugin test or claim to have tested inaccessible paid functionality.
- Review applicable licenses before incorporating third-party code into the public Rust project. Reference behavior is not automatic authorization to copy an implementation.
- Never commit secrets, database credentials, private site data, paid license keys or reference installation artifacts containing sensitive data.

Reference systems inform capabilities and comparisons. They do not dictate wpalt's internal design when a more integrated implementation better meets the product goals.

## 4. Feature-parity tracking

For every capability, record:

| Field | Required information |
|---|---|
| Identity | Stable capability ID, cluster and source/reference behavior. |
| Outcome | What the user can accomplish and the relevant constraints. |
| Dependencies | Shared services, integrations, host requirements and deployment limits. |
| Status | Planned, specified, implemented, verified, or explicitly deferred with reason. |
| Evidence | Relevant component, cluster and acceptance tests; applicable comparison and benchmark results. |
| Migration | Imported data, unsupported mappings, output differences and rollback considerations. |
| Completion limits | Known gaps, security concerns and operational caveats. |

Start with WordPress core and the research capability catalog. Refine large capability groups into meaningful behaviors. The research's 132 groups are planning units, not a sufficient acceptance suite by themselves.

Distinguish equivalent functionality, data migration, rendered-output compatibility and runtime/API compatibility. Do not label one as proof of another. WordPress PHP plugin and theme runtime compatibility is not part of the assumed scope.

## 5. Architecture and implementation process

Begin with an architecture proposal covering shared content models, permissions, composition/rendering, databases, media storage, jobs, configuration, diagnostics and deployment modes. Use decision records to capture alternatives and practical consequences.

Favor cohesive shared primitives and explicit module boundaries. Avoid both a collection of disconnected mini-products and a monolith where disabled features still impose unrelated work. Introduce distributed components when the validated deployment requirements justify them.

Deliver working vertical slices: each slice should connect storage, behavior, permissions, administration, public/API output where relevant, diagnostics and meaningful verification. A backend endpoint alone does not complete a user-facing capability.

Use the following lifecycle for every milestone:

1. **Prepare:** assess the previous release and perform justified refactoring or optimization that enables the next outcome. Protect existing behavior with relevant checks; record what changed and why. Keep preparation bounded to the milestone's needs.
2. **Specify:** define the substantial end-user outcome, supported configurations, acceptance journeys, failures and measurable completion criteria. Break the work into functional steps. Establish relevant performance and resource budgets from baseline measurements; record the rationale and any unresolved targets.
3. **Make each step work:** implement a correct, usable vertical slice, including applicable permissions, validation and failure handling. First establish working behavior; do not defer basic security or data integrity until an optimization pass.
4. **Verify and optimize each step:** use component and interaction tests, profile the affected paths, address relevant security and concurrency issues, and improve user experience. Record evidence before moving the step to verified status. Prefer measured improvements to speculative tuning.
5. **Integrate and optimize the whole milestone:** exercise complete journeys and earlier milestone regressions. Review combined queries, caching, jobs, memory/disk footprint, security boundaries, concurrency, recovery and UX. Step-level evidence alone does not prove the integrated system.
6. **Deliver and review:** produce the runnable release, reproducible verification instructions, concise evidence, known limitations and updated parity matrix. Create the milestone pull request and link it in the progress record. Mark the milestone complete only when its user outcome and agreed optimization/hardening criteria pass and its delivery PR exists; distinguish verification, review and merge status.

The [milestone roadmap](wpalt-milestones.md) defines the cumulative systems. Internal improvements support those systems and do not count as separate milestone deliveries. The user can review intermediate acceptance journeys as well as the final application; do not add a permission pause for every routine implementation step.

Select modern frontend and application tools through explicit decisions. Keep the Rust backend and deployment/footprint requirements central to those choices.

## 6. Meaningful test design

Tests must establish useful behavioral guarantees, expose realistic failures or protect against regressions. Avoid exhaustive duplication, assertions that merely mirror implementation, fragile snapshots without a clear contract, and test counts used as a quality metric.

Use the smallest test layer that can verify a guarantee reliably:

- **Component tests:** meaningful isolated rules, state transitions, validation, serialization and permission decisions.
- **Integration tests:** actual database behavior, migrations, storage, job processing and service boundaries.
- **Cluster tests:** features sharing data and workflows, such as publishing with SEO and caching, or forms with consent and queued mail.
- **High-level acceptance tests:** complete user journeys through the application, written so the user can understand and review the expected behavior.

Not every feature needs a large test collection at every layer. Map requirements to the few tests that convincingly establish them. Share setup and reusable fixtures while keeping each test's purpose visible.

Use descriptive names and explicit arrange/action/outcome structure. Prefer assertions about observable behavior. Keep deterministic data, controllable time where needed, bounded waits and useful failure output. Do not conceal flaky tests with indefinite retries.

## 7. High-level acceptance suite

Maintain a concise, representative suite suitable for the user's final review. Expand it as features mature. Initial journey categories include:

- Install and configure a local site; create an administrator; inspect effective configuration safely.
- Define structured content, compose a theme, preview a draft and publish it with correct URLs and SEO output.
- Upload and process media; confirm public access and restricted access behave correctly.
- Submit a conditional form, persist the entry and execute queued actions with consent and permission checks.
- Create restricted content and verify access through the admin panel, API and public routes.
- Exercise commerce, membership and booking workflows as those clusters are implemented.
- Back up a site, recover into a fresh installation and verify content, media, configuration and permissions.
- Run supported deployment/database combinations and confirm representative workflows produce equivalent outcomes.
- Operate with optional provider connections disabled and confirm unrelated local features continue working.

Include selected failure journeys, not only successful demonstrations. The final review should see the behavior, expected result and any remaining limitation rather than needing to interpret internal test code.

## 8. Database correctness and optimization

Run relevant database integration and migration tests against both SQLite and PostgreSQL. Use each database's real constraints and transaction behavior; a mock database or SQL string assertion cannot prove them.

- Define schema constraints, indexes and transaction boundaries alongside the behavior they protect.
- Inspect query plans and measure critical queries on representative data volumes.
- Track query counts and address N+1 access, unnecessary reads, unbounded pagination and avoidable contention.
- Verify sorting and pagination contracts, including concurrent changes where the workflow requires guarantees.
- Test supported migrations, failure handling and recovery, including data-bearing upgrades.
- Document differences between engines and deployment modes rather than forcing misleading uniform behavior.

Optimize deliberately for workload and correctness. An optimization must not weaken authorization, consistency or recoverability. Re-measure changed critical paths; do not repeatedly run unrelated expensive suites without a reason.

## 9. Performance and footprint verification

Define reproducible workloads before choosing numerical budgets. Measure relevant latency distributions, throughput, process memory, storage growth, installation footprint, query counts and background job cost.

Compare wpalt with recorded WordPress/plugin configurations using equivalent content, enabled capabilities, hardware and cache conditions. Include both cold and warm behavior where meaningful. Separate idle costs, active workloads and optional worker costs. State measurement limitations.

Profile request and query paths before speculative optimization. Include large datasets, concurrent requests, slow dependencies, bounded queues and resource pressure where relevant. Verify that disabled modules avoid unnecessary work.

Use small repeatable performance checks during development and focused deeper benchmarks at appropriate milestones. Avoid noisy timing assertions in ordinary unit tests. Publish baselines and regressions in a form that can be reproduced.

## 10. Security verification

Maintain a threat model as the architecture evolves. Each sensitive capability must have verification appropriate to its risks, including:

- Authentication, session lifecycle, authorization and privilege boundaries.
- Access control consistency across administration, APIs, public rendering and background jobs.
- Input validation, SQL injection prevention, output escaping and request forgery protections where applicable.
- File upload paths, traversal, archive extraction, media processing and resource limits.
- Template/composition execution, extension permissions and unsafe capabilities.
- SSRF risks in fetchers, webhooks, imports and network integrations.
- Secrets, debug logs, sensitive content, configuration exports and backups.
- Rate limits, abuse handling and operational access.

Use negative tests for unauthorized and malformed operations. Incorporate dependency and static checks where useful, and investigate their findings rather than treating a clean scanner result as proof of security. Record unresolved material risks explicitly.

## 11. Concurrency, races and distributed failure

Define the invariants that concurrent execution must preserve, such as inventory bounds, booking capacity, unique identifiers, permission changes, job ownership and complete publication state.

Test relevant simultaneous operations with controlled coordination rather than hoping a random stress run triggers a race. Verify transaction conflicts, retries, cancellation, duplicate requests, idempotency and crash boundaries. Test multi-instance behavior for the supported deployment modes.

Exercise worker termination, partial writes, database unavailability, slow integrations and lease expiry where applicable. Document the delivery guarantees of queues and side effects. Do not claim exactly-once external execution without an end-to-end mechanism that actually establishes it.

## 12. Recovery and operational testing

Restore into a fresh environment rather than treating successful archive creation as proof of a backup. Check database consistency, files, configuration, encryption key requirements and supported version compatibility. Test interruption and integrity failures where relevant.

Verify jobs can recover according to documented guarantees and that health reporting reflects failures. Exercise configuration validation, secret redaction, upgrade behavior and deployment instructions. Keep restore and migration processes understandable and repeatable.

## 13. Development checks and completion

Use formatting, compilation, linting and targeted tests routinely. Add broader integration and acceptance runs when changes cross boundaries or a milestone requires them. Establish continuous integration that provides useful evidence without running every expensive workload on every edit.

A capability is verified when its relevant behavior, permissions, configuration, failures and interactions have evidence; its user-facing surface and documentation are complete; and material limitations are recorded. Relevant performance or security claims require corresponding measurements or checks.

Maintain progress honestly: report what changed, why, what was verified and what remains. Never equate a passing test count with full feature parity, and never describe a staged implementation as the finished system while required work remains.

## 14. Initial sequence

Use the [milestone roadmap](wpalt-milestones.md) as the initial sequence. Repository setup, the architecture proposal, reference installations and the capability matrix are preparation for the first usable publishing system, rather than a separate internal-only milestone.

Refine milestone contracts when dependencies and reference behavior become clear. Preserve the end-user outcome and full-goal traceability. Before declaring the final system complete, reconcile all WordPress core and selected locally implementable plugin requirements with implementation and verification evidence; document and realign on any remaining scope gaps.

The sequence may change through documented decisions. Material product-scope changes require explicit agreement and coordinated updates to the durable requirements.

## 15. Feature-guidance freshness protocol

Apply this protocol to every feature. Record an internal behavioral contract for features without external guidance; record the authoritative external basis where it exists. Do not manufacture an external standard for an internal design choice.

1. **Discover and verify:** consult current primary documentation at feature specification and implementation time. For SEO, use applicable search-engine guidance and structured-data specifications; for other features use the relevant standards body, platform/provider documentation, security guidance or official database/framework documentation. Check applicability, publication/revision information and version scope. Distinguish mandatory requirements, recommendations and inference.
2. **Record:** maintain a repository evidence manifest with feature ID, source URLs, version/revision when available, last-checked date, applicability, mapped requirements/tests, responsible maintenance role, risk-based review interval and next review date. Record uncertainty or conflicting guidance explicitly. Prefer links and concise derived requirements to copying source documents.
3. **Implement and verify:** map the applicable guidance to behavior and tests. Include representative generated-output checks where useful. Validate against actual supported versions; do not accept unchanged plugin output as proof that guidance is current.
4. **Review changes:** check the manifest before the affected milestone is declared complete and before releases; review on its recorded schedule and on relevant guideline revisions, vulnerabilities, API deprecations or dependency-version changes. Development/release tooling should flag overdue records and unsupported versions and report the affected features. Automated link/change checks assist review; a successful HTTP request does not verify semantic correctness.
5. **Assess and decide:** record the change, affected rules/workflows, severity and migration implications. Update implementation, fixtures and evidence; make an explicit decision about unresolved changes. Realign product-direction changes with the user. Prioritize security-critical changes appropriately rather than waiting for an ordinary milestone.
6. **Release and maintain:** ship reviewed rules with a versioned release and changelog. Use independently versioned update bundles only where suitable, with integrity/authenticity checks, validation and rollback. Provide an offline/manual path. Never silently download executable policy or unreviewed guidance into production.

Use a lightweight manifest checker in development/CI/release tooling to enforce required fields and flag stale evidence. The concrete format and risk-based intervals are chosen during M1 preparation. External review happens in the maintenance workflow; deployed sites continue operating with bundled rules without mandatory internet access. Maintenance records state how current the release is rather than claiming continuous conformity to every future guideline.

## 16. Milestone pull requests

Create a primary delivery PR for each of M1–M9 on a milestone branch. A draft may open early and evolve into the verified delivery; smaller supporting PRs are allowed when useful but do not replace the milestone PR.

Each milestone PR describes the resulting end-user system and includes:

- Its milestone ID, scope and linked requirements/parity entries.
- A runnable demonstration and understandable acceptance journeys.
- Relevant test, performance, security, concurrency and UX evidence, with limitations.
- Current guidance/evidence review results for the affected features.
- Breaking changes, migration/reset instructions, upgrade limits and compatibility removals.
- Updated planning, operational and decision documents.

Record the PR URL, verification status, review status and merge status separately. A PR's existence is not proof of correctness. Ensure each milestone still has one coherent, reviewable delivery when preparatory refactors or multiple supporting changes are involved.

## 17. Breaking changes without permanent compatibility debt

Before M9, optimize for the current architecture rather than preserving every earlier milestone interface. Update callers, tests, themes and configuration together. Protect end-user capabilities through updated acceptance contracts; do not accumulate old endpoints, flags, fallback queries or parallel models solely to keep pre-adoption versions working.

For every breaking change, document the affected formats/versions and classify persisted data as meaningful or disposable. Provide a targeted migration when preservation is necessary, or an explicit reset/reseed process for disposable development fixtures. Tests should exercise the current design and any migration actually supported, rather than retaining an expanding matrix of obsolete milestone behavior.

At M9 and for future adopted versions, maintain a supported upgrade matrix. Migrations should include preflight validation, a recovery/backup strategy, recorded progress, consistency checks and defined interruption/retry behavior. Cover database, configuration, media references, theme/composition formats and relevant integration contracts. State whether rollback is reversible or requires restoring a backup; do not promise reversible destructive migrations.

Prefer one-off migration tooling or bounded upgrade bridges over legacy behavior in normal requests. Register any unavoidable shim with its owner, reason, version range, migration prerequisite and removal release/review deadline. Add a development/release check for overdue shims and review them during milestone preparation and release work. Remove obsolete runtime code, feature flags, old branches and redundant tests once the supported migration path permits it. Preserve or archive required migration tools and fixtures so support claims remain true.

For distributed upgrades, document either a short supported mixed-version transition or a controlled maintenance window. Do not maintain indefinite cross-version execution simply to avoid an explicit upgrade procedure.
