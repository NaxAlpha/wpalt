# M9 high-level acceptance review

These journeys test user-visible authority and failure outcomes rather than mirroring implementation functions. The cumulative Rust suite contains 88 tests; applicable acceptance/business journeys repeat against real SQLite and PostgreSQL. Browser, CLI, process, reference-model and delivery checks are additional connected journeys, not included in that count. No line/branch coverage percentage has been measured.

| Owner/user journey | Expected result | Readable verification source |
|---|---|---|
| Draft, autosave, preview and publish | Private work stays private; reviewed publication and restoration preserve content | `tests/acceptance.rs`: `author_preview_publish_autosave_and_restore_without_leaking_drafts` |
| Two editors or application nodes write | One reviewed version wins; conflict is explicit; withdrawn cached publication disappears | `concurrent_editors_never_silently_overwrite_each_other`; `scripts/local_process_acceptance.py` |
| Native themes and composed layouts | Typed bindings use published data; drafts/history do not corrupt content | `typed_models_and_reusable_components_render_only_published_data`; `theme_drafts_publish_restore_and_switch_without_content_loss` |
| Read and edit the administration interface | Consistent controls, keyboard/accessible geometry, responsive populated business workflows | `scripts/browser_acceptance.cjs`, `scripts/ui_contracts.cjs`, `scripts/operations_acceptance.cjs` |
| Withdraw/regrant privacy consent with delayed offer replies | Old replies cannot resurrect withdrawn offers; current allowed offer remains usable | `scripts/cookie_scan.cjs`; `scripts/consent_generation_acceptance.cjs` |
| Sign in with a passkey through alternating nodes | Both ceremonies can cross nodes; replay is rejected; private challenge state is shared | `scripts/browser_acceptance.cjs`; local-process transport evidence |
| Compete for the last product or booking place | Exactly one order admits capacity; duplicate receipt cannot allocate twice; retry returns the accepted order | `scripts/local_process_acceptance.py`: three independent races per domain |
| Retry the same form across nodes | Independent anti-abuse proofs do not create two entries for one idempotency key | `scripts/local_process_acceptance.py`: three form races |
| Kill a mutating process or worker | Committed graph remains authoritative; unresolved work pauses all nodes until explicit owner reconciliation | `scripts/local_process_acceptance.py`: controlled SQL locks, native mutation intent and three worker kills |
| Stop a worker while its stage is blocked | Signal requests finite drain; clean completion records success; interruption does not masquerade as success | `scripts/local_process_acceptance.py`: SIGTERM with blocked stage; `background_history_reports_failure_interruption_and_blocks_unrecorded_dispatch` |
| Upgrade a meaningful old installation | Old runtime refuses unsupported data; encrypted pre-upgrade point exists; current upgrade preserves graph; old binary can restore into fresh rollback target | `scripts/maintenance_acceptance.py`: published actual M8 executable in Linux CI |
| Physically recover PostgreSQL elsewhere | Native directory mismatch refuses ordinary start; reviewed stopped-source rebind holds recovered site until account/credential/queue review | `scripts/native_pitr_acceptance.py`; `scripts/maintenance_acceptance.py` |
| Transfer configuration to a fresh installation | Default export redacts secrets; secret admission is explicit/private; stale plans and overwrites fail | `scripts/config_transfer_acceptance.py` |
| Observe independently hosted sites | One outage or revoked grant does not hide healthy nodes; no token/content appears in report or redirected request | `scripts/fleet_acceptance.py` |
| Audit publication and prepare translations | Graph reads canonical public snapshots; exact-plan synchronization preserves translated text/publication; model proposal stays an unscheduled human-reviewed draft | New `content_audit_...` and `translation_duplication_...` Rust journeys; `scripts/integration_acceptance.py`; `scripts/local_ai_reference.py` |
| Receive a clean distributable build | Compiler-floor/security/native recovery/frontend/process gates pass; source identity/member bytes/checksums verified independently | `.github/workflows/m1.yml`; `scripts/release_build.py` |

## Evidence interpretation

Current local process and cumulative browser runs pass. Actual installed Qwen3 1.7B preserves one synthetic notice's facts but needs language editing; the smaller model failed quality. Repeated resource results and final exact-source GitHub artifact are separate gates tracked in the progress ledger. A passing inventory slice is not universal plugin compatibility. See [capability reconciliation](m9-capability-reconciliation.md) for retained scope decisions.

Local-process execution deliberately uses conservative serialized admission, explicit crash pause and stopped maintenance. It is not automatic failover, online mixed-version rolling writes or proof of shared-filesystem safety across physical hosts. Independent fleet observation must run outside the observed origin host to detect that host's outage.
