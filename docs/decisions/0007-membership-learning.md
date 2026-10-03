# 0007 — Shared local membership policy and versioned learning

Accepted implementation decision, 2026-10-03. M5 preserves the agreed membership/learning outcome.

Use indexed relational policy, entitlement/grant/group/resource records beside shared users/content/media. Anonymous SQL selection excludes protected publications before pagination or serialization; direct authenticated delivery evaluates the authoritative policy. Optional module disable hides workflow routes and denies protected resources rather than exposing them. Existing content rendering and bounded media delivery remain shared.

Courses have a bounded declarative draft/live definition. Stable lesson identities and explicit publication versions make quiz answers, assignment review and progress unambiguous. Re-publication does not silently reinterpret old grades. Reuse document/content authoring rather than inventing a second rich-text format. SQL uniqueness and idempotency preserve attempts/completions; no cached entitlement snapshot becomes authority. Current single-owner mutation coordination is retained without asserting M9 distributed guarantees.

Schema/archive version 8 is a forward data-bearing upgrade from merged M4 v7; old archives are restored with their matching runtime, upgraded, then re-exported. No permanently maintained v7 runtime parser. New tables and derived indexes install transactionally; pre-existing users/content remain unchanged. Local entitlement grants expose an origin reference for later M6 purchase reconciliation, which cannot grant privileges without the shared policy service.

The current primary basis is OWASP authorization guidance (deny by default and evaluate every request), WCAG/WAI form feedback, and supported PostgreSQL/SQLite transaction/index behavior. Feature evidence records map these requirements to tests and review deadlines.
