# Frontend foundation contract

Date: 2026-10-01. Supporting PR before M3; not a replacement product milestone. M1 and M2 are merged. M3 remains planned.

## End-user outcome

Consistent, legible administration with clear focus/error/saving/disabled states, predictable control sizes and spacing, and usable authoring/Studio layouts at narrow and wide widths. This preparation supports future product screens rather than introducing a new theme marketplace or design-product milestone.

## Source of truth and extension protocol

`frontend/design-tokens.json` defines versioned semantic colors, typography, four-pixel spacing, radii, target sizes and layout measures. `frontend/admin-ui.css` consumes generated `--ui-*` variables; `frontend/build.mjs` bundles that sheet into `assets/generated/admin-ui.css`. The server embeds it and loads it only for admin/sign-in. Public package tokens and theme rendering remain their own M2 contract. No runtime Node, cloud fonts or mandatory accounts.

`frontend/ui.jsx` provides native Button, Field, Notice and responsive Disclosure primitives. Buttons default to `type=button` so adding a field/repeater inside a form cannot accidentally submit it. Explicit form submit behavior remains supported. Fields connect labels, help/errors and invalid state. Busy/disabled controls and alerts have semantic states; the Studio uses these primitives. Server-rendered forms consume the same CSS contract. Adopt these primitives for new interactive features rather than inventing unrelated controls; preserve native semantics instead of rebuilding selects/buttons in divs.

A new component must name its user purpose, states, token choices, keyboard interaction, narrow layout and important failure behavior. Extend the representative gallery/cluster only for a new meaningful interaction or risk. Do not add every prop permutation, thousands of screenshots or assertions that simply copy token values. Review component and integrated screen together.

## Precise measurement contract

`scripts/ui-contracts.json` is an independently reviewed product contract, separate from token generation. Current values: normal action/input/select targets at least 44 CSS pixels high, control radius 6, open-section radius 0, desktop sidebar 224, workspace padding 32 desktop / 16 mobile, visible focus outline at least 3 pixels. Our 44px product choice is stricter than WCAG 2.2 AA's 24px target criterion, which includes exceptions. Geometry permits 0.5px rounding; horizontal document overflow is rejected. Tables have an explicit local scroll exception, not a page-wide exemption. Preview has a usable minimum width.

`scripts/ui_contracts.cjs` runs within the existing real-browser fixture. It records component bounding boxes, radius, font size/line height, names, viewport and failures in `work/ui-review/measurements.json`. Nine actual admin routes (overview, library, new editor, Studio, settings, media, comments, operations, people/access) run at 320/768/1440px; Studio also checks 701/1201px just above layout transitions. Below 768px its panes stack to keep the preview usable, including when the sidebar is still fixed. The shared component gallery covers primary/secondary/destructive/disabled/busy, help/error fields, select/check, notices, disclosure and status. Keyboard focus, obscuration at the target center, invalid state and real field editing are checked. An injected undersized action must trigger a failure, proving the guard detects the regression it claims to protect. Contrast checks use computed solid sRGB colors, with 4.5:1 normal text / 3:1 meaningful input boundary. Axe-core 4.13.0 scans the gallery and nine desktop admin routes against its WCAG A/AA tagged rules, records incomplete/manual findings, and excludes theme iframe contents from this admin-only scope; these focused samples do not certify the entire application's accessibility.

Real screen PNGs are review artifacts. The deterministic gallery additionally compares against reviewed platform/browser PNG baselines using pixelmatch, including dimension equality; threshold 0.15 per-pixel color distance, antialias differences excluded, at most 0.1% changed pixels. These tolerances are local regression choices, not aesthetic scores. Font/rasterization/platform differences need separate baselines; pin Playwright 1.62.1 and Ubuntu 24.04 in CI. The report includes full page screenshots with optional control-box overlays, position/dimension tables and the raw geometry/style records. Page readiness, asset bytes and initial layout shifts are observations rather than flaky timing assertions; no field Core Web Vitals claim. macOS Chrome is separate and records its actual version; its baseline is useful locally but not a universal cross-machine promise. CI never automatically approves new baselines.

From the repository root:

```sh
npm ci --prefix frontend --ignore-scripts
npm --prefix frontend run build
cargo build --release --locked
frontend/node_modules/.bin/playwright install chromium
WPALT_BINARY="$PWD/target/release/wpalt" \
  PLAYWRIGHT_MODULE="$PWD/frontend/node_modules/playwright" \
  node scripts/browser_acceptance.cjs
```

Linux CI uses Playwright's `install --with-deps chromium` to provide the browser's OS dependencies. The entire npm test/build dependency graph, including Playwright, is locked and audited. To use your installed Chrome locally, additionally set `CHROME_EXECUTABLE`; its actual version is recorded and must match the intended local baseline environment. For an intentional design revision, run locally with `UI_UPDATE_BASELINES=1`; inspect gallery plus actual-page screenshots and measurement deltas, then commit baseline changes with rationale. For Linux bootstrap, retrieve CI's actual gallery images from `ui-review`, inspect them, and explicitly copy them into `tests/ui-baselines/linux-chromium/`; rerun normal comparison. Missing baselines fail, rather than silently passing. Build/verification/dependency tools are development-only. PR CI uploads measurements, actual and diff images on success or failure; successful artifact packaging still requires all checks.

## Human aesthetic and UX review

Automation checks known constraints and visual change. A reviewer must assess hierarchy, density, line wrapping, alignment/optical balance, clarity of labels, task effort, feedback and responsive composition. Check representative populated and failure states, not empty showcase beauty alone. Read screenshots at actual CSS scale and try keyboard/edit flows. Record intentional changes, remaining limitations and evidence; don't report "aesthetics passed" because pixels match. Default/hover/focus/error/loading/disabled and long labels should be inspected when affected.

Future milestones extend this foundation. M3 adds multilingual direction/long translation and discovery-field contracts; M4–M6 add their workflow controls; M8 covers extension/theme-author tooling; M9 includes cross-browser/device and broader assistive-technology acceptance. No existing theme requirement is dropped or new milestone invented. Runtime admin theme customization, localization and full accessibility conformance are not delivered by this supporting PR.

## Current guidance

Reviewed 2026-10-01: [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html), [non-text contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html), [focus not obscured](https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html), [Playwright visual comparisons](https://playwright.dev/docs/test-snapshots). Standards requirements and informative guidance are distinguished from our internal dimensions/tolerances. Renew through the feature-guidance freshness gate at affected release or guidance/browser changes.

Status: locally verified implementation; [supporting PR #4](https://github.com/NaxAlpha/wpalt/pull/4) records current-head CI and review status. [Verification evidence](evidence/frontend-foundations-verification.md) and the baseline review record explain results and limitations.

The native Studio token disclosure starts collapsed on narrow screens to reduce initial scrolling; desktop remains expanded. Active navigation preserves the parent Content section in the editor. Inline prose links have an underline so color is not their only identification.

The default admin design is specified in [Calm admin design](calm-admin-design.md). Contract version 2 protects open sections without enclosing card backgrounds/borders/shadows, text-spacing reflow and reduced-motion behavior. This is a deliberate design-contract replacement before adoption; no theme/schema compatibility layer is needed.
