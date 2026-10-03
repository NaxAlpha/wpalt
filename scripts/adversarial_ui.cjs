// One populated cross-feature review, sharing the same design contracts as isolated workflows.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, origin, output) => {
  const page = await owner.newPage();
  const report = {
    status: "in-progress",
    surfaces: [],
    geometry: [],
    accessibility: [],
  };
  const roots = [
    "/admin/forms",
    "/admin/audience",
    "/admin/campaigns",
    "/admin/mail",
    "/admin/engagement",
    "/admin/engagement/catalog",
    "/admin/promotions",
    "/admin/registrations",
  ];
  const surfaces = new Set(roots);
  await page.route(origin + "/__ui_fixture/adversarial-spacing.css", (route) =>
    route.fulfill({
      contentType: "text/css",
      body: "body * { line-height:1.5!important; letter-spacing:.12em!important; word-spacing:.16em!important; } p { margin-bottom:2em!important; }",
    }),
  );
  await page.route(origin + "/__ui_fixture/axe.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fs.readFileSync(
        path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
      ),
    }),
  );
  try {
    // Discover populated detail pages from actual owner navigation, avoiding downloads and mutations.
    for (const root of roots) {
      await page.goto(origin + root);
      const links = await page
        .locator("a[href]")
        .evaluateAll((nodes) => nodes.map((n) => new URL(n.href).pathname));
      for (const pattern of [
        /^\/admin\/forms\/[^/]+$/,
        /^\/admin\/audience\/contacts\/[^/]+$/,
        /^\/admin\/campaigns\/[^/]+$/,
        /^\/admin\/mail\/[^/]+$/,
        /^\/admin\/promotions\/[^/]+$/,
      ]) {
        const link = links.find((p) => pattern.test(p));
        if (link) surfaces.add(link);
      }
      if (root === "/admin/forms") {
        const form = links.find((p) => /^\/admin\/forms\/[^/]+$/.test(p));
        if (form) {
          const listing = form + "/entries";
          surfaces.add(listing);
          await page.goto(origin + listing);
          const entry = await page
            .locator("a[href]")
            .evaluateAll((nodes) =>
              nodes
                .map((n) => new URL(n.href).pathname)
                .find((p) => /\/entries\/[^/]+$/.test(p)),
            );
          if (entry) surfaces.add(entry);
        }
      }
    }
    for (const surface of surfaces) {
      for (const width of [320, 768, 1440]) {
        await page.setViewportSize({ width, height: 1000 });
        const response = await page.goto(origin + surface);
        assert.equal(
          response.status(),
          200,
          `Populated owner screen ${surface}`,
        );
        await page.evaluate(async () => {
          await document.fonts.ready;
          await new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          );
        });
        const normal = await ui.geometry(page);
        report.geometry.push({ surface, state: "normal", ...normal });
        assert.deepEqual(
          normal.failures,
          [],
          `${surface}@${width}: ${normal.failures.join("\n")}`,
        );
        const spacing = await page.addStyleTag({
          url: origin + "/__ui_fixture/adversarial-spacing.css",
        });
        const spaced = await ui.geometry(page);
        report.geometry.push({
          surface,
          state: "user-text-spacing",
          ...spaced,
        });
        assert.deepEqual(
          spaced.failures,
          [],
          `${surface}@${width} text spacing: ${spaced.failures.join("\n")}`,
        );
        await spacing.evaluate((el) => el.remove());
        if (width === 320) {
          await ui.accessibility(page, origin, surface, report);
          await page.screenshot({
            path: path.join(
              output,
              "review-" + surface.replaceAll("/", "-") + "-320.png",
            ),
            fullPage: true,
            animations: "disabled",
          });
        }
      }
      report.surfaces.push(surface);
    }
    report.status = "passed";
    console.log(
      `PASS: populated adversarial UI review of ${surfaces.size} business screens at three widths, text spacing and narrow-screen accessibility.`,
    );
  } catch (error) {
    report.status = "failed";
    report.error = error.message;
    await page
      .screenshot({
        path: path.join(output, "adversarial-ui-failure.png"),
        fullPage: true,
      })
      .catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync(
      path.join(output, "adversarial-ui.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
    await page.close();
  }
};
