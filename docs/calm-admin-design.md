# Calm default administration

Date: 2026-10-01. Supporting redesign before M3, authorized by the user after frontend-foundation PR #4 merged. M3 is not started.

## Product direction

Current administration and sign-in should feel professional, composed and comfortable. Use a warm off-white canvas, charcoal type, a quiet light sidebar and one restrained blue-green accent. Open sections use whitespace and fine horizontal rules, rather than enclosing cards or elevation. Borders still identify editable fields; the preview keeps a frame to distinguish website output from the admin application. Status/errors retain text and semantics rather than relying on color.

Local system sans-serif fonts provide familiar readable forms and avoid font downloads/layout shifts from external font loading. Body text is 16px; controls and supporting interface text 14px; headings use measured 600-weight type and restrained tracking. Four-pixel spacing, 44px controls and visible keyboard focus continue. Control corners are 6px; section corners are zero. These are product judgments, not a standards-defined definition of premium design or a clinical stress-reduction claim.

Overview, content editing/listing, media, moderation, Studio, settings, access and operations share this contract. Navigation preserves the parent settings section for access management. Titles identify the task rather than using a different slogan on every screen. Studio tools use navigation-style emphasis; saving is the primary action, publication a distinct secondary action. Short pages fill the viewport with the same canvas. Native controls, server-rendered forms and Preact composition controls remain integrated with the existing permissions and data model.

## Verification and maintenance

Extend `scripts/ui-contracts.json` deliberately when design dimensions change. The real-browser cluster protects 32 measured surfaces: shared controls and nine existing admin routes at 320/768/1440px, plus Studio's 701/1201px boundary checks. It checks cardless section geometry, controls, placement, readable colors, selected sections, keyboard focus, errors, disabled/busy states and mobile disclosure. Representative text-spacing overrides and reduced-motion preferences are checked. Ten scoped automated accessibility scans and three explicitly reviewed platform-specific pixel references supplement the existing high-level authoring/publishing/media/moderation/theme journeys. These samples do not establish full accessibility conformance.

Inspect actual screenshots for hierarchy, comfortable density, wrapping, action priority and alignment; measurements and unchanged pixels cannot certify aesthetics. Inspect both Linux and macOS references before committing revisions. Baselines never approve themselves in CI. Record capture/version/hash provenance and preserve independent dimensions/behavior checks.

Normative and informative guidance was checked on 2026-10-01: [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [text spacing](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html), [contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html), [animation from interactions](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html). Disabling nonessential transitions for reduced motion is also our product policy; the animation criterion is AAA. Refresh through the UI-01 feature-guidance protocol when guidance, browsers or affected components change.

## Scope boundary

No new theme products, public theme redesign, localization/RTL, SEO, forms, commerce, extension ecosystem or new workflow capability is delivered here. Existing theme output in the preview stays under its own theme contract. Future milestones extend this coherent admin language and their own feature journeys. No backend schema, user-data migration, account requirement, runtime frontend service or added font dependency.
