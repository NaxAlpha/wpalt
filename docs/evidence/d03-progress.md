# D03 development evidence

2026-10-07 (Tokyo). Active implementation; not a completed delivery or localization claim.

The connected language workspace lists bounded content pages and compares saved linked variants with explicit content language/direction. Native preview/apply forms and JSON operations use the actual current editor session. A shared mutation guard now covers source/target loading, exact-plan validation and native save without recursively acquiring the guard. CLI stopped-site translation retains its own explicit owner boundary.

Protected-value synchronization requires every source access policy on the target, in addition to the existing refusal to duplicate protected sources into a new unprovisioned draft. Source/target version changes invalidate preview plans; publication, document, discovery and access remain independently authored. The editor links to the workspace; source variants are excluded from target choices and a site with one language explains that another configured language is needed.

`language_workspace_requires_current_actor_and_exact_protected_review` passes on real SQLite and PostgreSQL. It checks writer attribution, draft exclusion, repeat execution, private comparison, stale source plans, unchanged target revision after refusal, policy provisioning and revoked users. The existing CLI translation/synchronization journey also passes on both engines. Strict all-target Clippy passes. These checks do not yet establish browser geometry/accessibility, complete cumulative regression, optimized-request behavior or final release readiness.

Remaining D03: account interface-language preferences and scoped bundled catalogs; canonical long-document local proposals and actual model quality evaluation; recovery/migration of new persisted state; connected language/browser/RTL measurements; cumulative delivery gates and exact-source artifact verification. No D03 delivery PR exists yet.

D02's final-head clean Linux archive was independently verified at PR merge source `3030e67201a57bf924d1dea807c44cac0392b32f`: archive SHA-256 `1856305f9bade54816a0bbd95399916ee4e28f0a6526fb11496e543d2771a1d9`, executable 34,964,808 bytes. Final-head CI/merge state remains on PR #18 until all seven required jobs pass. D01 PR #17 is merged and its actual-main release `nightly-202610070048-051d319c7251` is independently verified.
