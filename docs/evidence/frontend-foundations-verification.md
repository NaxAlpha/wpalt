# Frontend foundation verification

Date: 2026-10-01. Local release verified; PR checks record current Linux verification. This is supporting preparation before M3, not a new product milestone or a full accessibility certification.

## Reviewable changes

One semantic token source feeds bundled admin/sign-in styling. Shared native actions/fields/notices/disclosures provide explicit state and labeling behavior. Normal control targets are at least 44px, text/control contrast has meaningful checks, spacing/radii/type are measured rather than judged solely through screenshots. Studio import no longer clips its file label, selected navigation has a persistent marker (including nested content routes), properties use page scrolling, mobile token disclosure reduces initial scroll, invalid fields/errors are explicit, and inline prose links have an underline.

[Desktop Studio](frontend/admin-builder-1440.png) · [320px Studio](frontend/admin-builder-320.png) · [local results](frontend-foundations-local.json) · [contract and reproduction](../frontend-foundations.md).

## Verification layers

- Twelve existing high-level acceptance journeys pass locally on SQLite; current PR CI requires actual SQLite and PostgreSQL 17. No storage schema change is made.
- Formatting/Clippy with warnings denied pass. Native release CLI install/configuration/scheduler/recovery/theme-portability/redacted-log checks pass.
- Native release Chrome journeys preserve publishing, autosave, revision, media, moderation, theme switching, reusable composition, typed authoring and invalid-feedback behavior; no unexpected external requests or browser script errors.
- UI cluster measures twenty surfaces: shared components plus five real admin routes at 320/768/1440px and Studio checks at 701/1201px. Independent contracts check target dimensions, radius, typography, workspace/sidebar placement, no document overflow, usable preview, keyboard focus and states, disclosure behavior and current section on nested routes. An injected undersized action proves detection.
- Three deterministic gallery visual references pass the ordinary comparator on macOS. Linux requires separately reviewed references; missing references deliberately fail rather than creating expected images automatically.
- Axe-core 4.13.0 runs six representative WCAG A/AA-tagged scans. Local scans report no violations or incomplete findings. Theme iframe contents are excluded from this admin-only check; no claim of full WCAG/assistive-technology conformance follows.
- `ui-review` CI artifact contains actual/diff PNGs, raw measurements and an HTML report with control overlays and position tables. Page-readiness/assets/initial layout shifts are measured observations, not timing assertions or field CWV scores.

## Review findings and interpretation

Measured checks caught a table-panel radius inconsistency. Automated accessibility checks caught a dashboard inline link distinguishable only by color. Both were corrected. Boundary review found that the Studio preview could become too narrow just above the mobile navigation breakpoint; the panes now stack below 768px and a 701px contract protects that case. Visual review found mobile tokens pushing the preview too far down; a native responsive disclosure now exposes them on demand. Desktop/mobile spacing, heading hierarchy, labels and form states were inspected through the real screenshots. These are documented judgments, not an automated aesthetic score.

Typography/system-font differences require platform-specific references. macOS baselines record native Chrome; Linux CI pins Ubuntu 24.04 and Playwright 1.62.1. Browser/font upgrades require inspection and explicit baseline updates. Pixel comparison tolerates antialias differences and up to 0.1% changed pixels; geometry/contrast contracts remain independent. Real populated pages produce review images and measurements without brittle full-page golden snapshots of random content IDs/order.

The Studio JS and new admin CSS are bundled with the Rust application; exact raw/gzip sizes and native executable size are in local results. Development-only axe/pixelmatch/pngjs add no runtime Node or network dependency. No new memory/disk-superiority claim or repeated database benchmark is warranted for this frontend-only change. Future multilingual, commerce, extension/theme and distributed milestones extend their own representative interactions through this foundation.

The initial [Linux capture](https://github.com/NaxAlpha/wpalt/actions/runs/36796239498) passed eighteen geometry surfaces and six accessibility scans, then deliberately failed only because its three reviewed references did not yet exist. The 320/768/1440px images were inspected for hierarchy, wrapping, states and error treatment and committed separately from macOS. [Baseline review record](../../tests/ui-baselines/review.json) records provenance and image hashes. Final PR-head checks remain authoritative.

Final focus review also identified that a light-panel blue focus color was insufficient on the dark sidebar. A separate semantic dark-surface focus color and an actual sidebar keyboard/contrast journey now protect both contexts. Development image inspection is recorded; user aesthetic review/approval remains pending on the PR.
