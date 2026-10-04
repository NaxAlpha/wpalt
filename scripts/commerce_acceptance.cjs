// Connected merchant/customer commerce journeys with independent shared UI measurements.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, origin, output) => {
  const merchant = await owner.newPage();
  const context = await owner
    .browser()
    .newContext({ viewport: { width: 1440, height: 1000 } });
  const shopper = await context.newPage();
  const report = {
    journey: [],
    measurements: [],
    accessibility: [],
    script_errors: [],
    remote_requests: [],
  };
  const submit = async (p, button) =>
    Promise.all([p.waitForNavigation({ waitUntil: "load" }), button.click()]);
  const measure = async (p, name) => {
    for (const width of [320, 768, 1440]) {
      await p.setViewportSize({ width, height: 1000 });
      const geometry = await ui.geometry(p);
      report.measurements.push({ surface: name, state: "normal", ...geometry });
      assert.deepEqual(geometry.failures, [], `${name} ${width}`);
      await p.screenshot({
        path: path.join(output, `commerce-${name}-${width}.png`),
        fullPage: true,
      });
      const spaced = await p.addStyleTag({
        url: origin + "/__ui_fixture/commerce-spacing.css",
      });
      const spacing = await ui.geometry(p);
      report.measurements.push({
        surface: name,
        state: "text-spacing",
        ...spacing,
      });
      assert.deepEqual(spacing.failures, [], `${name} spaced ${width}`);
      await spaced.evaluate((n) => n.remove());
    }
    await p.addScriptTag({ url: origin + "/__ui_fixture/commerce-axe.js" });
    const a11y = await ui.accessibility(p);
    report.accessibility.push({ surface: name, ...a11y });
    assert.deepEqual(a11y.violations, [], `${name} accessibility`);
  };
  for (const p of [merchant, shopper]) {
    p.on("pageerror", (e) => report.script_errors.push(e.message));
    p.on("request", (r) => {
      if (!r.url().startsWith(origin + "/"))
        report.remote_requests.push(r.url());
    });
    await p.route(origin + "/__ui_fixture/commerce-axe.js", (r) =>
      r.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(
          path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
        ),
      }),
    );
    await p.route(origin + "/__ui_fixture/commerce-spacing.css", (r) =>
      r.fulfill({
        contentType: "text/css",
        body: "body * {line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important} p {margin-bottom:2em!important}",
      }),
    );
  }
  try {
    const password = crypto.randomBytes(20).toString("hex");
    await merchant.goto(origin + "/admin/users");
    const account = merchant.locator("form").filter({
      has: merchant.getByRole("button", {
        name: "Create account",
        exact: true,
      }),
    });
    await account.getByLabel("Name", { exact: true }).fill("M6 Shopper");
    await account
      .getByLabel("Email", { exact: true })
      .fill("m6-shopper@example.test");
    await account
      .getByLabel("Role", { exact: true })
      .selectOption("subscriber");
    await account
      .getByLabel("Initial password", { exact: true })
      .fill(password);
    await submit(
      merchant,
      account.getByRole("button", { name: "Create account", exact: true }),
    );
    await merchant.goto(origin + "/admin/shop");
    await merchant
      .locator("summary")
      .filter({ hasText: "Create product" })
      .click();
    const create = merchant.locator("form").filter({
      has: merchant.getByRole("button", {
        name: "Create product",
        exact: true,
      }),
    });
    await create
      .getByLabel("Product title", { exact: true })
      .fill("M6 Field notebook");
    await create.getByLabel("URL slug", { exact: true }).fill("m6-notebook");
    await create
      .getByLabel("Product type", { exact: true })
      .selectOption("physical");
    await create
      .getByLabel("Description", { exact: true })
      .fill("A locally managed physical purchase with clear prices.");
    await create.getByLabel("Publish product", { exact: true }).check();
    await submit(
      merchant,
      create.getByRole("button", { name: "Create product", exact: true }),
    );
    const productId = merchant.url().split("/").pop();
    const variant = merchant.locator("form").filter({
      has: merchant.getByRole("button", {
        name: "Create variant",
        exact: true,
      }),
    });
    await variant
      .getByLabel("Variant title", { exact: true })
      .fill("Notebook — blue");
    await variant
      .getByLabel("Unique SKU", { exact: true })
      .fill("M6-BROWSER-NOTEBOOK");
    await variant.getByLabel("Price", { exact: true }).fill("18.00");
    await variant
      .getByLabel("Total physical stock (-1 for nonphysical)", { exact: true })
      .fill("3");
    await submit(
      merchant,
      variant.getByRole("button", { name: "Create variant", exact: true }),
    );
    await measure(merchant, "product-editor");
    await merchant.goto(origin + "/admin/shop");
    await measure(merchant, "merchant");
    report.journey.push(
      "Merchant publishes a physical product and stock-controlled variant.",
    );
    await shopper.goto(origin + "/login");
    await shopper
      .getByLabel("Email", { exact: true })
      .fill("m6-shopper@example.test");
    await shopper.getByLabel("Password", { exact: true }).fill(password);
    await submit(
      shopper,
      shopper.getByRole("button", { name: "Sign in", exact: true }),
    );
    await shopper.goto(origin + "/shop");
    await measure(shopper, "catalog");
    await shopper
      .getByRole("link", { name: "M6 Field notebook", exact: true })
      .click();
    await measure(shopper, "product");
    await shopper
      .getByRole("button", { name: "Add to cart", exact: true })
      .focus();
    await shopper.keyboard.press("Enter");
    await shopper.waitForURL(origin + "/shop/cart");
    await measure(shopper, "cart");
    await shopper
      .getByLabel("Shipping address", { exact: true })
      .fill("Synthetic browser-test delivery address");
    await submit(
      shopper,
      shopper.getByRole("button", { name: "Place order", exact: true }),
    );
    const order = shopper.url().split("/").pop();
    assert((await shopper.locator("main").innerText()).includes("awaiting"));
    await measure(shopper, "pending-order");
    report.journey.push(
      "Shopper uses keyboard cart submission, reviews authoritative totals and places an unpaid order.",
    );
    assert.equal(
      (await owner.request.get(origin + "/shop/orders/" + order)).status(),
      404,
      "Another account cannot read the shopper receipt.",
    );
    await merchant.goto(origin + "/admin/shop/orders/" + order);
    await merchant
      .getByLabel("Actual received payment reference", { exact: true })
      .fill("browser-actual-payment");
    await submit(
      merchant,
      merchant.getByRole("button", {
        name: "Record received payment",
        exact: true,
      }),
    );
    assert((await merchant.locator("main").innerText()).includes("paid"));
    await measure(merchant, "paid-order");
    await shopper.reload();
    assert((await shopper.locator("main").innerText()).includes("USD 18.00"));
    await measure(shopper, "customer-receipt");
    await merchant
      .getByRole("button", { name: "Mark fulfilled", exact: true })
      .click();
    await merchant.waitForLoadState("load");
    await merchant.getByText("Request refund", { exact: true }).click();
    await merchant
      .getByLabel("Actual refund amount", { exact: true })
      .fill("18.00");
    await merchant
      .getByLabel("Refund reason", { exact: true })
      .fill("Browser customer return");
    await merchant
      .getByLabel("Restock physical items (final full refund only)", {
        exact: true,
      })
      .check();
    await submit(
      merchant,
      merchant.getByRole("button", { name: "Authorize refund", exact: true }),
    );
    await merchant
      .getByLabel("Actual refund payment reference", { exact: true })
      .fill("browser-actual-refund");
    await submit(
      merchant,
      merchant.getByRole("button", {
        name: "Record completed refund",
        exact: true,
      }),
    );
    assert((await merchant.locator("main").innerText()).includes("refunded"));
    await shopper.reload();
    assert((await shopper.locator("main").innerText()).includes("refunded"));
    report.journey.push(
      "Merchant records actual payment, fulfills, authorizes a full refund and restocks only after recording its completion.",
    );
    await merchant.goto(origin + "/admin/shop");
    await merchant.evaluate(() =>
      document.querySelectorAll("main details").forEach((d) => (d.open = true)),
    );
    await measure(merchant, "merchant-expanded");
    await shopper.goto(origin + "/shop/orders");
    await measure(shopper, "order-history");
    await shopper.goto(origin + "/shop/subscriptions");
    await measure(shopper, "subscriptions-empty");
    assert.equal(report.script_errors.length, 0);
    assert.equal(report.remote_requests.length, 0);
    report.product_id = productId;
    report.order_id = order;
  } finally {
    fs.writeFileSync(
      path.join(output, "commerce-result.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
    await context.close();
    await merchant.close();
  }
};
