// Few representative UI contracts, not a snapshot for every possible prop combination.
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const { PNG } = require("../frontend/node_modules/pngjs");
const contract = require("./ui-contracts.json");
const root = path.resolve(__dirname, "..");
const output = path.join(root, "work/ui-review");

async function geometry(page) {
  // Locator visibility after navigation does not establish stylesheet/font readiness.
  // Measure a completed rendered document, retaining every geometry assertion.
  await page.waitForLoadState("load");
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  return page.evaluate((c) => {
    const failures = [],
      components = [];
    const near = (a, b) => Math.abs(a - b) <= c.geometry_tolerance_px;
    const check = (ok, message) => {
      if (!ok) failures.push(message);
    };
    const rect = (el) => {
      const r = el.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    };
    const visible = (el) =>
      el.getClientRects().length &&
      getComputedStyle(el).visibility !== "hidden";
    check(
      document.documentElement.scrollWidth <= innerWidth + 1,
      "document horizontal overflow",
    );
    for (const el of document.querySelectorAll(
      "button, .button, input:not([type=hidden]):not([type=checkbox]):not([type=radio]), select",
    )) {
      if (!visible(el)) continue;
      const r = rect(el),
        style = getComputedStyle(el);
      const name =
        el.getAttribute("aria-label") ||
        el.labels?.[0]?.textContent.trim() ||
        el.textContent.trim() ||
        el.tagName;
      components.push({
        kind: el.tagName.toLowerCase(),
        name: name.slice(0, 80),
        ...r,
        radius: parseFloat(style.borderRadius),
        font: style.fontSize,
        line_height: style.lineHeight,
      });
      check(
        r.width + c.geometry_tolerance_px >= c.control_min_width,
        `${name}: target too narrow`,
      );
      check(
        near(parseFloat(style.fontSize), c.control_font_px),
        `${name}: control typography drift`,
      );
      check(
        r.height + c.geometry_tolerance_px >= c.control_min_height,
        `${name}: target shorter than ${c.control_min_height}px`,
      );
      check(
        near(parseFloat(style.borderRadius), c.control_radius),
        `${name}: inconsistent control radius`,
      );
      // Tables have explicit local overflow; a page-wide overflow is never acceptable.
      if (!el.closest(".table-wrap"))
        check(
          r.x >= -1 && r.x + r.width <= innerWidth + 1,
          `${name}: clipped horizontally`,
        );
    }
    for (const el of document.querySelectorAll(".panel")) {
      const s = getComputedStyle(el);
      check(
        near(parseFloat(s.borderRadius), c.panel_radius),
        "section radius drift",
      );
      check(
        parseFloat(s.borderLeftWidth) === 0 &&
          parseFloat(s.borderRightWidth) === 0 &&
          parseFloat(s.borderBottomWidth) === 0 &&
          s.boxShadow === "none" &&
          s.backgroundColor === "rgba(0, 0, 0, 0)",
        "section must stay open without an enclosing card",
      );
    }
    check(
      near(
        parseFloat(getComputedStyle(document.body).fontSize),
        c.body_font_px,
      ),
      "body typography drift",
    );
    const side = document.querySelector(".sidebar");
    if (side && innerWidth > 700)
      check(near(rect(side).width, c.sidebar_width), "sidebar width drift");
    const main = document.querySelector(".workspace");
    if (main) {
      const expected =
        innerWidth <= 700
          ? c.mobile_workspace_padding
          : c.desktop_workspace_padding;
      check(
        near(parseFloat(getComputedStyle(main).paddingLeft), expected),
        "workspace padding drift",
      );
      if (side && innerWidth > 700)
        check(near(rect(main).x, c.sidebar_width), "workspace/sidebar overlap");
    }
    const iframe = document.querySelector(".preview-panel iframe");
    if (iframe) {
      const r = rect(iframe);
      check(r.width >= 200, "preview narrower than usable content");
    }
    return {
      viewport: innerWidth,
      document_width: document.documentElement.scrollWidth,
      layout: {
        main: main ? rect(main) : null,
        sidebar: side ? rect(side) : null,
      },
      surfaces: [
        ...document.querySelectorAll(
          ".panel,.notice,.status,h1,h2,label,textarea,fieldset,details,summary,iframe,.outline,.binding,.studio-toolbar,.studio-tabs,.sidebar nav a",
        ),
      ]
        .filter(visible)
        .map((el) => {
          const s = getComputedStyle(el);
          return {
            kind: el.tagName.toLowerCase(),
            classes: el.className,
            ...rect(el),
            font_size: s.fontSize,
            line_height: s.lineHeight,
            padding: s.padding,
            margin: s.margin,
            color: s.color,
            background: s.backgroundColor,
            border: s.border,
            radius: s.borderRadius,
          };
        }),
      components,
      failures,
    };
  }, contract);
}
function luminance(rgb) {
  const n = rgb
    .map((v) => v / 255)
    .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * n[0] + 0.7152 * n[1] + 0.0722 * n[2];
}
function contrast(a, b) {
  const x = luminance(a),
    y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
async function palette(page) {
  const pairs = await page.evaluate(() => {
    const rgb = (s) =>
      s
        .match(/[\d.]+/g)
        .slice(0, 3)
        .map(Number);
    const out = [];
    for (const [selector, name] of [
      ["[data-ui=primary]", "primary action"],
      [".ui-field label", "field label"],
      [".help", "supporting text"],
      [".notice.error", "error message"],
      ["[data-ui=title]", "field boundary"],
    ]) {
      const el = document.querySelector(selector),
        s = getComputedStyle(el);
      let bg = el;
      while (bg && getComputedStyle(bg).backgroundColor === "rgba(0, 0, 0, 0)")
        bg = bg.parentElement;
      out.push({
        name,
        color: rgb(name === "field boundary" ? s.borderTopColor : s.color),
        background: rgb(getComputedStyle(bg).backgroundColor),
        boundary: name === "field boundary",
      });
    }
    return out;
  });
  return pairs.map((p) => ({
    ...p,
    ratio: contrast(p.color, p.background),
    minimum: p.boundary
      ? contract.control_boundary_contrast
      : contract.normal_text_contrast,
  }));
}
async function screenshot(page, name, platform) {
  await page.evaluate(() => document.fonts.ready);
  const actual = path.join(output, name + ".png");
  await page.screenshot({
    path: actual,
    fullPage: true,
    animations: "disabled",
    caret: "hide",
  });
  const baseline = path.join(
    root,
    "tests/ui-baselines",
    platform,
    name + ".png",
  );
  if (process.env.UI_UPDATE_BASELINES === "1") {
    assert(
      !process.env.CI,
      "Baseline updates are explicit local review operations, never ordinary CI.",
    );
    fs.mkdirSync(path.dirname(baseline), { recursive: true });
    fs.copyFileSync(actual, baseline);
    return { name, baseline_updated: true };
  }
  assert(
    fs.existsSync(baseline),
    `Missing reviewed ${platform} baseline: ${name}. See docs/frontend-foundations.md.`,
  );
  const a = PNG.sync.read(fs.readFileSync(actual)),
    b = PNG.sync.read(fs.readFileSync(baseline));
  assert.equal(a.width, b.width, `${name}: screenshot width changed`);
  assert.equal(a.height, b.height, `${name}: screenshot height changed`);
  const diff = new PNG({ width: a.width, height: a.height });
  const { default: pixelmatch } =
    await import("../frontend/node_modules/pixelmatch/index.js");
  const count = pixelmatch(a.data, b.data, diff.data, a.width, a.height, {
    threshold: contract.visual_diff.threshold,
    includeAA: contract.visual_diff.include_anti_alias,
  });
  fs.writeFileSync(path.join(output, name + "-diff.png"), PNG.sync.write(diff));
  const ratio = count / (a.width * a.height);
  assert(
    ratio <= contract.visual_diff.max_changed_ratio,
    `${name}: ${(ratio * 100).toFixed(3)}% changed pixels; inspect actual/baseline/diff before updating.`,
  );
  return { name, changed_pixels: count, changed_ratio: ratio };
}
async function accessibility(page, origin, surface, results) {
  await page.addScriptTag({ url: origin + "/__ui_fixture/axe.js" });
  const scan = await page.evaluate(async () => {
    const result = await axe.run(
      { exclude: [["iframe"]] },
      {
        runOnly: {
          type: "tag",
          values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"],
        },
      },
    );
    const concise = (items) =>
      items.map((v) => ({
        rule: v.id,
        impact: v.impact,
        nodes: v.nodes.map((n) => ({
          target: n.target,
          summary: n.failureSummary || "manual review needed",
        })),
      }));
    return {
      violations: concise(result.violations),
      incomplete: concise(result.incomplete),
      passed_rules: result.passes.length,
    };
  });
  results.accessibility.push({
    surface,
    viewport: page.viewportSize().width,
    ...scan,
  });
  assert.deepEqual(
    scan.violations,
    [],
    `${surface}: automated accessibility violations ${JSON.stringify(scan.violations)}`,
  );
}
module.exports = async function verifyUI(context, origin) {
  fs.mkdirSync(output, { recursive: true });
  const page = await context.newPage();
  const results = {
    contract_version: contract.version,
    platform: process.platform,
    browser: context.browser().version(),
    geometry: [],
    visual: [],
    contrast: [],
    accessibility: [],
    performance: [],
    visual_failures: [],
  };
  const platform =
    process.env.UI_BASELINE_PLATFORM || `${process.platform}-chromium`;
  await page.addInitScript(() => {
    window.__uiCls = 0;
    window.__uiObserver = new PerformanceObserver((list) => {
      for (const e of list.getEntries())
        if (!e.hadRecentInput) window.__uiCls += e.value;
    });
    window.__uiObserver.observe({ type: "layout-shift", buffered: true });
  });
  try {
    await page.route(origin + "/__ui_fixture", (route) =>
      route.fulfill({
        contentType: "text/html",
        body: `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>UI reference</title><link rel="stylesheet" href="/assets/app.css"><link rel="stylesheet" href="/assets/admin-ui.css"><body class="admin"><div id="gallery"></div><script src="/__ui_fixture/gallery.js"></script></body></html>`,
      }),
    );
    await page.route(origin + "/__ui_fixture/gallery.js", (route) =>
      route.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(path.join(root, "work/ui-gallery.js")),
      }),
    );
    await page.route(origin + "/__ui_fixture/axe.js", (route) =>
      route.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(
          path.join(root, "frontend/node_modules/axe-core/axe.min.js"),
        ),
      }),
    );
    await page.route(origin + "/__ui_fixture/text-spacing.css", (route) =>
      route.fulfill({
        contentType: "text/css",
        body: "body * { line-height: 1.5 !important; letter-spacing: .12em !important; word-spacing: .16em !important; } p { margin-bottom: 2em !important; }",
      }),
    );
    for (const width of contract.viewports) {
      await page.setViewportSize({ width, height: contract.height });
      await page.goto(origin + "/__ui_fixture");
      await page
        .getByRole("button", { name: "Save draft", exact: true })
        .waitFor();
      const m = await geometry(page);
      results.geometry.push({ surface: "shared components", ...m });
      assert.deepEqual(m.failures, [], m.failures.join("\n"));
      const colors = await palette(page);
      results.contrast.push({ viewport: width, pairs: colors });
      for (const p of colors)
        assert(p.ratio >= p.minimum, `${p.name}: contrast ${p.ratio}`);
      if (width === 320)
        await accessibility(page, origin, "shared components", results);
      try {
        results.visual.push(
          await screenshot(page, `components-${width}`, platform),
        );
      } catch (error) {
        results.visual_failures.push(error.message);
      }
    }
    // Calm interaction must respect the system's reduced-motion preference.
    await page.emulateMedia({ reducedMotion: "reduce" });
    const reduced = await page
      .locator("[data-ui=primary]")
      .evaluate((el) => getComputedStyle(el).transitionDuration);
    assert.equal(
      reduced,
      "0s",
      "Reduced-motion preference must disable transitions",
    );
    results.reduced_motion = "passed";
    await page.emulateMedia({ reducedMotion: "no-preference" });
    // User text-spacing overrides must not hide or clip the component gallery.
    await page.setViewportSize({ width: 320, height: contract.height });
    const spacing = await page.addStyleTag({
      content:
        ".ui-gallery * { line-height: 1.5 !important; letter-spacing: .12em !important; word-spacing: .16em !important; } .ui-gallery p { margin-bottom: 2em !important; }",
    });
    const spaced = await geometry(page);
    assert.deepEqual(
      spaced.failures,
      [],
      "User text spacing must preserve usable targets and reflow: " +
        spaced.failures.join("; "),
    );
    results.text_spacing = "passed";
    await spacing.evaluate((el) => el.remove());
    await page.setViewportSize({ width: 1440, height: contract.height });
    // Keyboard/focus and state contracts use real components, not DOM mocks.
    await page.getByRole("button", { name: "Save draft", exact: true }).focus();
    await page.keyboard.press("Tab");
    assert.equal(await page.locator(":focus").textContent(), "Preview");
    const focus = await page.locator(":focus").evaluate((el) => ({
      width: parseFloat(getComputedStyle(el).outlineWidth),
      style: getComputedStyle(el).outlineStyle,
      color: getComputedStyle(el)
        .outlineColor.match(/[\d.]+/g)
        .slice(0, 3)
        .map(Number),
      background: (() => {
        let parent = el.parentElement;
        while (
          parent &&
          getComputedStyle(parent).backgroundColor === "rgba(0, 0, 0, 0)"
        )
          parent = parent.parentElement;
        return getComputedStyle(parent)
          .backgroundColor.match(/[\d.]+/g)
          .slice(0, 3)
          .map(Number);
      })(),
      rect: el.getBoundingClientRect().toJSON(),
      center:
        document.elementFromPoint(
          el.getBoundingClientRect().x + el.clientWidth / 2,
          el.getBoundingClientRect().y + el.clientHeight / 2,
        ) === el,
    }));
    assert(
      focus.width >= contract.focus_outline_min_width &&
        focus.style !== "none" &&
        focus.center,
      "Keyboard focus is invisible or obscured",
    );
    results.keyboard_focus = {
      ...focus,
      contrast: contrast(focus.color, focus.background),
    };
    assert(
      results.keyboard_focus.contrast >= contract.control_boundary_contrast,
      "Keyboard focus contrast is insufficient",
    );
    assert(
      await page
        .getByRole("button", { name: "Unavailable", exact: true })
        .isDisabled(),
    );
    assert.equal(
      await page
        .getByRole("button", { name: "Saving…", exact: true })
        .getAttribute("aria-busy"),
      "true",
    );
    const title = page.getByRole("textbox", { name: "Title", exact: true });
    await title.fill("A real edit");
    assert.equal(await title.inputValue(), "A real edit");
    assert.equal(
      await page
        .getByRole("textbox", { name: "Slug", exact: true })
        .getAttribute("aria-invalid"),
      "true",
    );
    await page.getByText("Revision details", { exact: true }).click();
    assert(
      await page
        .getByText("A recoverable working draft.", { exact: true })
        .isVisible(),
    );
    // Fault injection demonstrates that size/placement regression detection actually works.
    await page.locator("[data-ui=primary]").evaluate((el) => {
      el.style.minHeight = "1px";
      el.style.height = "10px";
      el.style.padding = "0";
      el.style.lineHeight = "8px";
    });
    assert(
      (await geometry(page)).failures.some((f) => f.includes("target shorter")),
      "Geometry guard failed to detect an injected broken control",
    );
    for (const route of [
      "/admin",
      "/admin/posts",
      "/admin/posts/new",
      "/admin/builder",
      "/admin/settings",
      "/admin/discovery",
      "/admin/discovery/links",
      "/admin/media",
      "/admin/comments",
      "/admin/operations",
      "/admin/users",
    ]) {
      const widths =
        route === "/admin/builder"
          ? [...new Set([...contract.viewports, 701, 1201])].sort(
              (a, b) => a - b,
            )
          : contract.viewports;
      for (const width of widths) {
        await page.setViewportSize({ width, height: contract.height });
        await page.goto(origin + route);
        if (route === "/admin/posts/new") {
          await page
            .getByText("Structured content fields", { exact: true })
            .waitFor({ state: "attached" });
          await page
            .getByText("Typed fields & composition", { exact: true })
            .click();
          await page
            .getByText("Structured content fields", { exact: true })
            .waitFor();
        }
        if (route === "/admin/builder")
          await page
            .getByRole("button", { name: "Save draft", exact: true })
            .waitFor();
        if (route === "/admin/builder")
          await page
            .frameLocator("iframe")
            .getByRole("heading", { name: "The Local Journal", exact: true })
            .waitFor();
        if (route === "/admin/builder" && width === 320) {
          const toggle = page.getByText("Design tokens", { exact: true });
          assert(
            !(await page.getByLabel("accent", { exact: true }).isVisible()),
            "Mobile token controls should start collapsed",
          );
          await toggle.click();
          assert(await page.getByLabel("accent", { exact: true }).isVisible());
          await toggle.click();
        }
        await page.evaluate(async () => {
          await document.fonts.ready;
          scrollTo(0, 0);
        });
        const m = await geometry(page);
        results.geometry.push({ surface: route, ...m });
        const timing = await page.evaluate(() => {
          const n = performance.getEntriesByType("navigation")[0];
          return {
            observed_ui_ready_ms: performance.now(),
            dom_content_loaded_ms: n.domContentLoadedEventEnd,
            layout_shift: window.__uiCls,
            assets: performance
              .getEntriesByType("resource")
              .filter((r) => r.name.includes("/assets/"))
              .map((r) => ({
                path: new URL(r.name).pathname,
                transfer_bytes: r.transferSize,
                decoded_bytes: r.decodedBodySize,
              })),
          };
        });
        results.performance.push({
          surface: route,
          viewport: width,
          ...timing,
        });
        const expectedNav = route.startsWith("/admin/posts")
          ? "Content"
          : {
              "/admin": "Overview",
              "/admin/builder": "Design studio",
              "/admin/settings": "Site settings",
              "/admin/discovery": "Discovery",
              "/admin/discovery/links": "Discovery",
              "/admin/users": "Site settings",
              "/admin/media": "Media library",
              "/admin/comments": "Comments",
              "/admin/operations": "Operations",
            }[route];
        assert.equal(
          await page
            .getByRole("link", { name: expectedNav, exact: true })
            .getAttribute("aria-current"),
          "page",
          "Current section must remain indicated on nested routes",
        );
        if (route === "/admin" && width === 1440) {
          await page.locator(".sidebar .brand").focus();
          await page.keyboard.press("Tab");
          const dark = await page.locator(":focus").evaluate((el) => {
            const rgb = (c) =>
              c
                .match(/[\d.]+/g)
                .slice(0, 3)
                .map(Number);
            const style = getComputedStyle(el);
            return {
              name: el.textContent.trim(),
              color: rgb(style.outlineColor),
              background: rgb(
                getComputedStyle(document.querySelector(".sidebar"))
                  .backgroundColor,
              ),
              width: parseFloat(style.outlineWidth),
            };
          });
          results.sidebar_keyboard_focus = {
            ...dark,
            contrast: contrast(dark.color, dark.background),
          };
          assert(
            dark.name === "Overview" &&
              dark.width >= contract.focus_outline_min_width &&
              results.sidebar_keyboard_focus.contrast >=
                contract.control_boundary_contrast,
            "Sidebar keyboard focus must remain visible and contrasting",
          );
          await page.locator(":focus").evaluate((el) => el.blur());
        }
        if (width === 1440) await accessibility(page, origin, route, results);
        assert.deepEqual(
          m.failures,
          [],
          `${route}@${width}: ${m.failures.join("\n")}`,
        );
        const spacingOverride = await page.addStyleTag({
          url: origin + "/__ui_fixture/text-spacing.css",
        });
        const spaced = await geometry(page);
        results.geometry.push({
          surface: route,
          state: "user-text-spacing",
          ...spaced,
        });
        assert.deepEqual(
          spaced.failures,
          [],
          `${route}@${width} with user text spacing: ${spaced.failures.join("\n")}`,
        );
        await spacingOverride.evaluate((el) => el.remove());
        await page.screenshot({
          path: path.join(
            output,
            route.split("/").filter(Boolean).join("-") + `-${width}.png`,
          ),
          fullPage: true,
          animations: "disabled",
          caret: "hide",
        });
      }
    }
    assert.deepEqual(
      results.visual_failures,
      [],
      results.visual_failures.join("\n"),
    );
    results.status = "passed";
    console.log(
      "PASS: measured component shapes/targets/contrast, keyboard states, regression guard, and eleven real admin surfaces at three widths.",
    );
  } catch (error) {
    results.status = "failed";
    results.error = error.message;
    await page
      .screenshot({ path: path.join(output, "failure.png"), fullPage: true })
      .catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync(
      path.join(output, "measurements.json"),
      JSON.stringify(results, null, 2) + "\n",
    );
    require("./ui_review_html.cjs")(results, output);
    await page.close();
  }
};

// Reuse the independently reviewed measurements in later integrated workflows.
module.exports.geometry = geometry;
module.exports.accessibility = accessibility;
