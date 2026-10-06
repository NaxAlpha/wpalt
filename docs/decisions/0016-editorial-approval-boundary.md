# 0016 — Bind editorial approval to current content and authority

Date: 2026-10-06 (Tokyo). Status: implementation decision within resumed D02 scope; verification pending.

Use a declarative per-model review requirement and relational current workflow/decision history, separate from immutable content revisions. Preserve existing broad editor/admin roles; assigned review and publication policy do not imply a restricted contributor role. D13 retains that broader authority work. Enabling review on legacy work requires a current save before request so the system does not invent its last editor.

Approval authorizes canonical meaningful material under the current relevant schema/policy and current active reviewer. Version-only approval is insufficient because publication requests carry editable values. A different last editor/requester/reviewer authority, content/definition edit, reassignment, restored revision or revocation requires review again. Scheduled promotion repeats the authority/payload check and preserves any previous live snapshot when it cannot promote safely.

Use explicit graph/version migration for the new domain tables. Do not hide mutable review state inside historical content snapshots solely to avoid a migration. Ordinary runtime and restore accept only their current format; stopped maintenance retains a verified original pre-change recovery point, and explicit offline converters have documented removal criteria. Tables, indexes, bounded history and privacy/selection recovery validation are required parts of the implementation.

Local queue/notifications work without external accounts. Optional email is an owner-configured existing transport, not an excuse to introduce cloud dependency. Reviewable delivery and later scope remain evidence-gated under the D02 contract.
