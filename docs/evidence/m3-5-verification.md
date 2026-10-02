# M3.5 verification record

2026-10-02. Delivery under verification; this record is not a production-readiness or full WordPress/Notion parity claim.

The implementation connects one canonical tree to the direct editor, public rendering, working/live snapshots, optimistic saves, schedules, revisions, backups and imports. It bundles a locked ProseMirror engine, supports contextual formatting and slash/block controls, integrates uploaded-image selection, and preserves early native input and explicit browser recovery.

Local checks so far: Rust format, warnings-denied Clippy, 19 high-level SQLite acceptance clusters plus two document trust/import clusters. Existing publishing, discovery, permissions, scheduling, media and recovery journeys remain active. New clusters cover structured publication/projection isolation and stale saves, safe backup/revision recovery, and transactional upgrade rollback followed by an operator correction/retry. PostgreSQL is mandatory in CI; local results alone do not claim it.

Real Chrome authoring and measured UI verification are being finalized. The cluster deliberately verifies changed block order and preserved source text, not just a button click or node count. It covers formatting/links, slash blocks, tables, image selection, early typing before enhancement, paste sanitation, failed saves/recovery, stale tabs, and 1,000-paragraph input. The shared UI suite retains independently reviewed gallery baselines and geometry/accessibility samples. Native OS IME, screen readers and broader browser/device certification remain explicit M9 hardening work; supplied Japanese/Arabic text is not an IME certification.

Dependency review found zero selected Rust advisory vulnerabilities/warnings and zero npm graph vulnerabilities. The one inactive optional Rust lockfile advisory remains excluded by the actual selected-feature graph, without a blanket ignore. The final PR must verify its exact source revision and downloaded clean artifact, including the new editor hash.

Update this record with final release measurements, visual review and current-head CI before marking the PR ready.
