# M4 delivery verification

2026-10-03. [PR #8](https://github.com/NaxAlpha/wpalt/pull/8) implements the defined [M4 contract](../m4-contract.md). Review/merge is separate. Current-head CI and exact clean-artifact evidence below determine delivery acceptance; earlier checkpoint passes do not prove later edits. This is not complete premium-plugin parity or a production-readiness certification.

## Reviewable outcomes and tests

| Outcome | Meaningful evidence |
|---|---|
| Visual form → immutable publication → authoritative response/actions → private follow-up | `publication_retries_and_fresh_restore_preserve_one_authoritative_entry`; hidden/derived rule clusters; `business_acceptance.cjs` |
| Conditions, repeats, steps, scores and statement acknowledgment | `hidden_values_and_forged_totals_cannot_enter_the_authoritative_submission`; `invalid_dependencies_and_shared_repeater_limits_fail_before_actions`; `surveys_scores_and_signature_acknowledgments_preserve_published_meaning` |
| Protected files, token search, notes, assignment, guarded export and fresh restore | `attachments_follow_up_and_search_remain_private_and_survive_fresh_restore` |
| Server partial recovery, explicit device storage, offline retry and UTF-8 upload | `partial_drafts_preserve_valid_work_without_bypassing_final_validation`; `workflows_acceptance.cjs` |
| Consent → confirmation → segments → durable queue → failure/withdrawal/deletion | `consent_confirmation_withdrawal_and_mail_recovery_are_one_durable_journey`; `confirmed_subscribers_receive_matching_trigger_once_and_withdrawal_cancels_it` |
| SMTP acknowledgment versus lost receipt | Owned loopback `smtp_acceptance_and_lost_receipts_have_distinct_recovery_states`; no third-party messages |
| Conditional notification, escaped moderated draft, mailbox proof and approved subscriber | `accepted_response_routes_once_creates_only_a_draft_and_requires_verified_approved_account`; `workflows_acceptance.cjs` |
| Shared content/theme form blocks, independent canonical composers and draft isolation | Full browser writing/builder regressions and workflow embedding journey; strict document/reference validation |
| No pre-consent collection, categories, geometry, GPC, withdrawal and storage recovery | `engagement_requires_current_consent_masks_geometry_and_erases_on_withdrawal`; `engagement_acceptance.cjs` |
| Stable variants, frequency and concurrent last-stock allocation | `local_offers_keep_variants_bound_frequency_and_allocate_last_reward_atomically`; owned popup keyboard/accessibility browser journey |
| Site-wide backpressure and rollback across independent pools | `site_wide_admission_is_atomic_replayable_and_preserves_unicode_bytes` |
| Populated response/event/queue operation and useful query plans | `populated_business_paths_have_bounded_queries_and_reproducible_measurements`; emitted `work/m4-volume.json` |

The Rust business suite is a small set of rules and connected journeys, supplemented by the 19 publishing acceptance clusters and two canonical-document trust clusters. Each database integration runs on real SQLite plus PostgreSQL 17 in required CI. Tests cover independent pools, admission/publication races, queue leases, contact deletion/recreation, capture/withdrawal and reward allocation; no process-local mutex substitutes for database proof.

Security boundaries include no visitor-chosen message destination or executable expressions; escaped draft contributions; proof-before-approval and subscriber permissions; private capability/attachment scope; stale definitions and payload-bound retries; declared categorical capture; public action redaction; bounded parser/file/row admission; current-consent delivery. Native no-referrer capability POSTs allow Origin:null only with exact same-origin navigation metadata on proof routes. Missing/same-site/cross-site metadata fails closed and account/admin writes keep their Origin/CSRF checks. [OWASP's current CSRF guidance](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html) informed this narrow boundary; HTTP and browser journeys verify it.

## Verification conditions and measurements

Local measurements use macOS native binaries, SQLite and synthetic disposable data. The volume harness uses the debug integration API, 1,000 authoritative responses with one fixed notification each, 2,000 consented events and real private fsync spooling. Timings are observations, with no unstable speed assertions. Query plans, submission distributions and queue throughput are retained as machine-readable evidence. Release HTTP/RSS/disk measurements use 1,000 stories plus About and composed theme output; debug diagnostics correlate request IDs with SQL counts without logging sensitive payloads. These are distinct scenarios, not universal production limits or a WordPress comparison.

Collection defaults off; ordinary public rendering obtains its optional analytics settings in the existing settings projection, without an additional analytics query. Business/engagement hard-disable removes the corresponding routes/assets and skips their jobs. Event/response/queue growth is explicitly bounded; full capacity rejects new admissions atomically while acknowledging accepted retries. Report aggregation still reads retained events; global write counters deliberately serialize admission. Future scale changes require measurement.

The full real-browser suite retains publishing, structured authoring, independent geometry/contrast/keyboard contracts, reviewed gallery pixel baselines and accessibility checks. Added narrow/desktop screens cover form designer, workflows, local offer composition/popups, privacy controls, playback and embedded forms. Screenshots and JSON measurements are review artifacts; automated geometry and axe do not prove subjective aesthetic quality.

## Recovery, limits and maintained guidance

Schema and backup 7 include private files, consent, queue, campaigns, partials, registration, contributions, geometry, local inventory and admission counters. Restore validates typed/frozen meanings and counters before importing, recovers network leases as uncertain and rebuilds local spools. Prior publishing data upgrades through one-time migrations; older archives restore with their matching binary first. No parallel legacy rendering/request path is introduced. See [operations](../operations.md), [business limits](../business.md) and ADR 0006.

The 25 feature-guidance records pass the maintenance checker as of 2026-10-03, with reviewed sources and scheduled updates. Current dependency audit, generated-asset reproducibility, warnings-denied Clippy, compiler floor and clean build are required. Guidance review does not certify universal legal compliance.

Coverage is defined individually in the 132-group parity matrix. Limits include finite floating-point calculations, typed-name statements rather than cryptographic signing, no PDF malware disarm, already-loaded offline forms rather than uncached offline navigation, confirmation-triggered templates rather than arbitrary automation graphs, a defined response/offer funnel, geometry rather than DOM replay, cookie rather than person identity, and owner SMTP rather than internet reputation. Commerce purchases/redemption remain M6. Accepted form records and historical backups have independent retention; audience deletion does not imply erasing those records.

## Delivery checks

Current-head PostgreSQL/browser CI, release measurements and downloaded clean-artifact source/hash verification are being completed. Do not infer readiness from this section until the final record identifies the passing run and verified archive.
