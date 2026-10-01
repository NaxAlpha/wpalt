# Calm admin verification

Date: 2026-10-01. See [design contract](../calm-admin-design.md) and the [frontend measurement protocol](../frontend-foundations.md).

The redesign applies the user-approved modern, cardless, calm direction to existing administration and sign-in. Local screenshot inspection checked open section hierarchy, labels/values, action priority, desktop alignment, narrow-screen wrapping and task consistency. This is development visual review; user aesthetic approval remains pending on the delivery PR.

## Evidence

- Twelve readable SQLite acceptance journeys pass; final PR CI also requires actual PostgreSQL 17. No storage behavior changes.
- Formatting, Clippy with warnings denied and evidence freshness checks pass.
- Existing real browser authoring/autosave/revision/media/moderation/theme journeys pass, with no external requests or script errors. Native release CLI configuration, install, recovery and redacted logging journeys pass.
- Thirty-two UI surfaces measure normal 44px targets, 6px control corners, open zero-radius sections without enclosing backgrounds/borders/shadows, placement, usable preview and overflow. Sidebar focus and parent settings navigation are checked on actual pages.
- Ten representative automated accessibility scans report no violations or incomplete findings locally; iframe theme contents remain excluded. Reduced-motion preference removes transitions. Text-spacing overrides preserve usable component targets and reflow.
- Three macOS gallery references were deliberately updated and inspected. Linux references require fresh capture, explicit image inspection and an ordinary passing comparison. Final PR CI is authoritative; a baseline-capture failure is not a ready delivery.

The current UI report contains screenshots, dimension overlays, position tables, contrast results and performance observations. The exact CSS/JS raw/gzip sizes and browser version are recorded in the companion JSON. No field Core Web Vitals, full accessibility certification, clinical stress-reduction or WordPress resource-superiority claim follows from these samples.

## Scope and review findings

Initial visual inspection found the old root background visible below short admin pages; the admin/auth body now fills the viewport. It also exposed competing save/publish emphasis and overly heavy editable values. The final hierarchy uses primary save/secondary publish and normal-weight field values, consistently across server and Studio controls. Boxed grouping is replaced by fine rules and space; editable controls and preview frames retain meaningful boundaries. Wider and mobile screenshots were inspected.

The date/version/primary-source freshness protocol remains UI-01. See [WCAG text-spacing guidance](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html) and [animation guidance](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html); motion AAA guidance is distinguished from our product policy. Future milestone products and public theme rendering stay in their existing contracts.
