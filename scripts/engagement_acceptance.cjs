// One integrated owner/visitor journey verifies composition, targeting and erasure.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const ui = require("./ui_contracts.cjs");
module.exports = async function (owner, origin, output) {
  const page = await owner.newPage();
  const context = await owner
    .browser()
    .newContext({ viewport: { width: 320, height: 900 } });
  const visitor = await context.newPage();
  const report = {
    measurements: [],
    accessibility: [],
    script_errors: [],
    remote_requests: [],
    journey: [],
  };
  for (const p of [page, visitor]) {
    p.on("pageerror", (e) => report.script_errors.push(e.message));
    p.on("request", (r) => {
      if (!r.url().startsWith(origin + "/"))
        report.remote_requests.push(r.url());
    });
    await p.route(origin + "/__ui_fixture/axe.js", (r) =>
      r.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(
          path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
        ),
      }),
    );
  }
  const submit = async (button) =>
    Promise.all([
      page.waitForNavigation({ waitUntil: "load" }),
      button.click(),
    ]);
  try {
    await visitor.goto(origin + "/");
    assert.equal(
      await visitor.locator("#engagement-controls").count(),
      0,
      "Fresh sites do not load capture or privacy UI.",
    );
    await page.goto(origin + "/admin/engagement");
    await page
      .getByLabel("Enable consented analytics", { exact: true })
      .check();
    await page
      .getByLabel("Offer optional masked interaction recording", {
        exact: true,
      })
      .check();
    await submit(
      page.getByRole("button", { name: "Save privacy settings", exact: true }),
    );
    await page.goto(origin + "/admin/promotions");
    await page
      .getByLabel("Promotion title", { exact: true })
      .fill("Local launch invitation");
    await submit(
      page.getByRole("button", { name: "Create promotion", exact: true }),
    );
    await page
      .getByLabel("Reward label", { exact: true })
      .fill("Preview invitation");
    await page.getByLabel("Available stock", { exact: true }).fill("2");
    await submit(page.getByRole("button", { name: "Add reward", exact: true }));
    for (const variant of ["A", "B"]) {
      const form = page
        .locator("form[data-editor]")
        .nth(variant === "A" ? 0 : 1);
      await form.locator(".ProseMirror").waitFor();
      await form.locator(".ProseMirror").click();
      await page.keyboard.type(`A calm invitation — variant ${variant}.`);
      await form.getByLabel("Activate", { exact: true }).check();
      await form
        .getByLabel("Compare stable A/B variants", { exact: true })
        .check();
      await form
        .getByLabel("Offer a weighted local reward draw", { exact: true })
        .check();
      await submit(
        form.getByRole("button", {
          name: `Save variant ${variant}`,
          exact: true,
        }),
      );
    }
    for (const width of [320, 1440]) {
      await page.setViewportSize({ width, height: 1000 });
      const measured = await ui.geometry(page);
      report.measurements.push({ surface: "offer-composition", ...measured });
      assert.deepEqual(measured.failures, [], `Offer layout at ${width}`);
      await page.screenshot({
        path: path.join(output, `local-offer-owner-${width}.png`),
        fullPage: true,
      });
    }
    await ui.accessibility(page, origin, "offer-composition", report);
    await visitor.goto(origin + "/");
    await visitor
      .getByRole("button", { name: "Allow local analytics", exact: true })
      .waitFor();
    assert.equal(
      (await context.cookies()).filter((c) => c.name === "wpalt_visitor")
        .length,
      0,
      "No visitor identifier before affirmative consent.",
    );
    await visitor
      .getByLabel("Also allow optional masked interaction recording", {
        exact: true,
      })
      .check();
    await visitor
      .getByRole("button", { name: "Allow local analytics", exact: true })
      .click();
    const dialog = visitor.getByRole("dialog", {
      name: "Local launch invitation",
      exact: true,
    });
    await dialog.waitFor();
    await dialog
      .getByRole("button", { name: "Draw a local reward", exact: true })
      .click();
    await dialog.getByRole("status").filter({ hasText: "LOCAL-" }).waitFor();
    await visitor.screenshot({
      path: path.join(output, "local-offer-visitor-320.png"),
      fullPage: true,
    });
    await ui.accessibility(visitor, origin, "local-offer-dialog", report);
    await visitor.keyboard.press("Escape");
    assert.equal(await dialog.count(), 0, "Native modal supports Escape.");
    const cookie = (await context.cookies()).find(
      (c) => c.name === "wpalt_visitor",
    );
    assert.ok(cookie?.httpOnly && cookie.sameSite === "Lax");
    const hash = crypto.createHash("sha256").update(cookie.value).digest("hex");
    await visitor.getByRole("link", { name: "Search", exact: true }).click();
    await visitor
      .getByLabel("Find something", { exact: true })
      .fill("PRIVATE_TYPED_SENTINEL");
    await visitor.locator("main h1").click();
    const frames = await owner.request.get(
      origin + `/api/admin/engagement/sessions/${hash}`,
    );
    assert.equal(frames.status(), 200);
    const payload = await frames.json();
    assert.ok(payload.frames.length > 0);
    assert.ok(
      !JSON.stringify(payload).includes("PRIVATE_TYPED_SENTINEL"),
      "Geometry never contains typed values.",
    );
    await page.goto(origin + `/admin/engagement/sessions/${hash}`);
    await page.locator("canvas").waitFor();
    await ui.accessibility(page, origin, "masked-playback", report);
    await page.screenshot({
      path: path.join(output, "masked-playback.png"),
      fullPage: true,
    });
    await visitor
      .getByRole("button", {
        name: "Withdraw and erase my analytics",
        exact: true,
      })
      .click();
    await visitor.getByText("Analytics declined.", { exact: false }).waitFor();
    const erased = await owner.request.get(
      origin + `/api/admin/engagement/sessions/${hash}`,
    );
    assert.deepEqual(
      (await erased.json()).frames,
      [],
      "Withdrawal removes all recorded frames.",
    );
    assert.equal(
      (await context.cookies()).filter((c) => c.name === "wpalt_visitor")
        .length,
      0,
    );
    await visitor.reload();
    await visitor
      .getByRole("button", { name: "Review privacy choices", exact: true })
      .waitFor();
    assert.equal(
      await visitor
        .getByRole("button", { name: "Allow local analytics", exact: true })
        .count(),
      0,
      "Decline remains quiet on later navigation.",
    );
    const gpc = await owner
      .browser()
      .newContext({ extraHTTPHeaders: { "Sec-GPC": "1" } });
    const gp = await gpc.newPage();
    await gp.goto(origin + "/");
    const status = await gpc.request.get(origin + "/api/engagement/status");
    assert.equal(
      (await status.json()).enabled,
      false,
      "The server honors GPC independently of script detection.",
    );
    await gpc.close();
    report.journey.push(
      "Owner composes both variants; consented visitor sees a keyboard-accessible targeted offer and draws once; wireframes omit inputs; withdrawal erases frames and cookie; GPC remains off.",
    );
    assert.deepEqual(report.script_errors, []);
    assert.deepEqual(report.remote_requests, []);
    fs.writeFileSync(
      path.join(output, "engagement.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
  } catch (error) {
    await page.screenshot({
      path: path.join(output, "engagement-owner-failure.png"),
      fullPage: true,
    });
    await visitor.screenshot({
      path: path.join(output, "engagement-visitor-failure.png"),
      fullPage: true,
    });
    throw error;
  } finally {
    await page.close();
    await context.close();
  }
};
