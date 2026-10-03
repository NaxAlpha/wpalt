// A single owner/visitor journey, not one test per control permutation.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async function formsJourney(owner, origin, output) {
  const page = await owner.newPage();
  const visitorContext = await owner
    .browser()
    .newContext({ viewport: { width: 320, height: 900 } });
  const visitor = await visitorContext.newPage();
  const report = {
    measurements: [],
    accessibility: [],
    journey: [],
    script_errors: [],
    remote_requests: [],
  };
  for (const current of [page, visitor]) {
    current.on("pageerror", (error) =>
      report.script_errors.push(error.message),
    );
    current.on("request", (request) => {
      if (!request.url().startsWith(origin + "/"))
        report.remote_requests.push(request.url());
    });
  }
  await page.route(origin + "/__ui_fixture/axe.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fs.readFileSync(
        path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
      ),
    }),
  );
  try {
    await page.goto(origin + "/admin/forms");
    await page.getByLabel("Title", { exact: true }).fill("Business enquiry");
    await Promise.all([
      page.waitForURL(/\/admin\/forms\/[^/]+$/),
      page.getByRole("button", { name: "Create form", exact: true }).click(),
    ]);
    const id = new URL(page.url()).pathname.split("/").at(-1);
    await page.getByLabel("Form title", { exact: true }).waitFor();
    await page
      .getByLabel("Maximum accepted responses", { exact: true })
      .fill("2");
    await page.getByLabel("Label", { exact: true }).nth(0).fill("Your name");
    await page.getByRole("button", { name: "Add field", exact: true }).click();
    await page
      .getByLabel("Label", { exact: true })
      .nth(1)
      .fill("Business customer");
    await page
      .getByLabel("Input", { exact: true })
      .nth(1)
      .selectOption("boolean");
    await page.getByRole("button", { name: "Add field", exact: true }).click();
    await page.getByLabel("Label", { exact: true }).nth(2).fill("Company");
    await page.getByLabel("Required", { exact: true }).nth(2).check();
    await page.locator("summary").nth(2).click();
    await page
      .getByLabel("Show when a previous field has a value", { exact: true })
      .nth(2)
      .selectOption({ label: "Business customer" });
    await page.getByLabel("Condition", { exact: true }).selectOption("Equal");
    await page.getByLabel("Value to match", { exact: true }).fill("true");
    await page
      .getByRole("button", { name: "Publish form", exact: true })
      .click();
    await page
      .getByRole("status")
      .filter({ hasText: "Published. Visitors" })
      .waitFor();
    for (const width of [320, 1440]) {
      await page.setViewportSize({ width, height: 1000 });
      const measured = await ui.geometry(page);
      report.measurements.push({ surface: "form-designer", ...measured });
      assert.deepEqual(
        measured.failures,
        [],
        `Form designer geometry at ${width}`,
      );
      await page.screenshot({
        path: path.join(output, `form-designer-${width}.png`),
        fullPage: true,
      });
    }
    await ui.accessibility(page, origin, "form-designer", report);
    await visitor.goto(origin + `/forms/${id}`);
    await visitor.getByLabel("Your name", { exact: true }).fill("Alice");
    assert.equal(
      await visitor.getByText("Loading form…", { exact: true }).count(),
      0,
      "The loading placeholder disappears when the form is usable.",
    );
    assert.equal(
      await visitor.getByLabel("Company", { exact: true }).count(),
      0,
      "Hidden required company is not an input.",
    );
    await visitor
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await visitor
      .getByRole("status")
      .filter({ hasText: "response has been received" })
      .waitFor();
    report.journey.push(
      "An unchecked business checkbox submits false and the hidden required company is discarded.",
    );
    await visitor.goto(origin + `/forms/${id}`);
    await visitor.getByLabel("Your name", { exact: true }).fill("Bob");
    await visitor.getByLabel("Business customer", { exact: true }).check();
    await visitor
      .getByLabel("Company", { exact: true })
      .fill("O'Brian <script>window.untrustedForm=true</script>");
    const geometry = await visitor.evaluate(() => ({
      width: innerWidth,
      document_width: document.documentElement.scrollWidth,
      controls: [
        ...document.querySelectorAll(
          "#public-form input:not([type=checkbox]),#public-form button",
        ),
      ].map((element) => ({
        height: element.getBoundingClientRect().height,
        radius: getComputedStyle(element).borderRadius,
      })),
    }));
    assert(geometry.document_width <= geometry.width + 1);
    assert(
      geometry.controls.every(
        (control) => control.height >= 43.5 && control.radius === "6px",
      ),
    );
    report.measurements.push({ surface: "visitor-form", ...geometry });
    await visitor.screenshot({
      path: path.join(output, "visitor-form-320.png"),
      fullPage: true,
    });
    await visitor
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await visitor
      .getByRole("status")
      .filter({ hasText: "response has been received" })
      .waitFor();
    await page.goto(origin + `/admin/forms/${id}/entries`);
    assert.equal(
      await page.getByRole("link", { name: /^Response / }).count(),
      2,
    );
    let found = false;
    const links = await page
      .getByRole("link", { name: /^Response / })
      .evaluateAll((elements) => elements.map((element) => element.href));
    for (const url of links) {
      await page.goto(url);
      if (
        await page
          .getByRole("heading", { name: "Company", exact: true })
          .count()
      ) {
        await page
          .getByText("O'Brian <script>window.untrustedForm=true</script>", {
            exact: true,
          })
          .waitFor();
        assert.equal(
          await page.evaluate(() => window.untrustedForm),
          undefined,
        );
        found = true;
      }
    }
    assert(
      found,
      "The owner sees the preserved company as text, with no executable markup.",
    );
    report.journey.push(
      "Two responses are visible only through the authenticated response viewer; supplied markup remains text.",
    );
    await visitor.goto(origin + `/forms/${id}`);
    await visitor
      .getByLabel("Your name", { exact: true })
      .fill("Third response");
    await visitor
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await visitor
      .getByRole("alert")
      .filter({ hasText: "response limit" })
      .waitFor();
    assert.equal(
      await visitor.getByLabel("Your name", { exact: true }).inputValue(),
      "Third response",
    );
    report.journey.push(
      "The configured response limit rejects further collection and preserves the visitor's typed work.",
    );
    const privateResponse = await visitorContext.request.get(
      origin + `/admin/forms/${id}/entries`,
      { maxRedirects: 0 },
    );
    assert.equal(privateResponse.status(), 303);
    assert.equal(privateResponse.headers()["location"], "/login");
    const privateData = await visitorContext.request.get(
      origin + `/api/admin/forms/${id}`,
    );
    assert.equal(privateData.status(), 401);
    assert.deepEqual(report.script_errors, [], "No form-script exceptions.");
    assert.deepEqual(
      report.remote_requests,
      [],
      "The form journey has no remote dependencies.",
    );
    fs.writeFileSync(
      path.join(output, "business-forms.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
  } catch (error) {
    await page.screenshot({
      path: path.join(output, "business-form-failure.png"),
      fullPage: true,
    });
    console.error(
      "Form workflow state:",
      await page.locator("main").innerText(),
    );
    throw error;
  } finally {
    await page.close();
    await visitorContext.close();
  }
};
