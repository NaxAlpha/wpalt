// One connected author/calculate/answer/report experience, with real browser evidence.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, visitors, origin, output) => {
  const page = await owner.newPage();
  const report = {
    status: "running",
    browser: owner.browser().version(),
    geometry: [],
    accessibility: [],
    assertions: [],
    errors: [],
  };
  page.on("pageerror", (error) => report.errors.push(error.message));
  await page.route(origin + "/__ui_fixture/axe.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: fs.readFileSync(
        path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
      ),
    }),
  );
  const submit = async (p, button) =>
    Promise.all([p.waitForNavigation({ waitUntil: "load" }), button.click()]);
  async function measure(name, admin = true) {
    for (const width of [320, 768, 1440]) {
      await page.setViewportSize({ width, height: 1000 });
      if (admin) {
        const measured = await ui.geometry(page);
        assert.deepEqual(measured.failures, []);
        report.geometry.push({ surface: name, ...measured });
      } else {
        await page.waitForLoadState("load");
        const dimensions = await page.evaluate(() => ({
          viewport: innerWidth,
          document_width: document.documentElement.scrollWidth,
        }));
        assert(dimensions.document_width <= dimensions.viewport + 1);
        report.geometry.push({
          surface: name,
          ...dimensions,
          scope:
            "Public layout overflow; precise admin geometry is a separate contract",
        });
      }
      await page.screenshot({
        path: path.join(output, `d10-${name}-${width}.png`),
        fullPage: true,
      });
      await page.screenshot({
        path: path.join(output, `d10-${name}-${width}-viewport.png`),
      });
    }
    await ui.accessibility(page, origin, name, report);
  }
  try {
    await page.goto(origin + "/admin/forms");
    await page
      .getByLabel("Title", { exact: true })
      .fill("D10 conditional quotation");
    await submit(
      page,
      page.getByRole("button", { name: "Create form", exact: true }),
    );
    const id = page.url().split("/").pop();
    await page.getByLabel("Form title", { exact: true }).waitFor();
    const csrf = await page.locator("#forms-studio").getAttribute("data-csrf");
    for (const [index, label, kind] of [
      [0, "Quantity", "number"],
      [1, "Rate", "number"],
      [2, "Total", "number"],
      [3, "Company", "string"],
    ]) {
      if (index)
        await page
          .getByRole("button", { name: "Add field", exact: true })
          .click();
      await page.getByLabel("Label", { exact: true }).nth(index).fill(label);
      await page
        .getByLabel("Input", { exact: true })
        .nth(index)
        .selectOption(kind);
      await page.getByLabel("Required", { exact: true }).nth(index).check();
    }
    const total = page.locator("section.panel").filter({
      has: page.getByRole("heading", {
        name: "Total",
        exact: true,
        level: 2,
      }),
    });
    await total
      .getByText("Visibility and calculation", { exact: true })
      .click();
    await total
      .getByLabel("Calculated value", { exact: true })
      .selectOption("Expression");
    await total
      .getByLabel("Expression operation", { exact: true })
      .nth(0)
      .selectOption("Round");
    await total.getByLabel("Decimal places", { exact: true }).fill("2");
    await total
      .getByLabel("Expression operation", { exact: true })
      .nth(1)
      .selectOption("Divide");
    await total
      .getByLabel("Expression operation", { exact: true })
      .nth(2)
      .selectOption("Multiply");
    await total
      .getByLabel("Expression operation", { exact: true })
      .nth(3)
      .selectOption("Field");
    await total
      .getByLabel("Expression field", { exact: true })
      .nth(0)
      .selectOption({ label: "Quantity" });
    await total
      .getByLabel("Expression operation", { exact: true })
      .nth(4)
      .selectOption("Field");
    await total
      .getByLabel("Expression field", { exact: true })
      .nth(1)
      .selectOption({ label: "Rate" });
    await total.getByLabel("Fixed number", { exact: true }).fill("3");
    const company = page.locator("section.panel").filter({
      has: page.getByRole("heading", {
        name: "Company",
        exact: true,
        level: 2,
      }),
    });
    await company
      .getByText("Visibility and calculation", { exact: true })
      .click();
    await company
      .getByLabel("Show when a previous field has a value", { exact: true })
      .selectOption({ label: "Total" });
    await company
      .getByLabel("Condition", { exact: true })
      .selectOption("Greater");
    await company.getByLabel("Value to match", { exact: true }).fill("10");
    await company.getByLabel("Condition", { exact: true }).selectOption("All");
    await company
      .getByRole("button", { name: "Add rule", exact: true })
      .click();
    await company
      .getByLabel("Rule field", { exact: true })
      .nth(1)
      .selectOption({ label: "Quantity" });
    await measure("composer");
    await page.getByText("Try the working form", { exact: true }).click();
    const preview = page.locator("details").filter({
      has: page
        .locator("summary")
        .getByText("Try the working form", { exact: true }),
    });
    await preview.getByLabel("Quantity", { exact: true }).fill("2");
    await preview.getByLabel("Rate", { exact: true }).fill("12.5");
    await preview.getByLabel("Quantity", { exact: true }).focus();
    await page.keyboard.press("Tab");
    assert.equal(
      await page.evaluate(() =>
        document.activeElement.labels[0].textContent.trim(),
      ),
      "Rate",
      "Keyboard follows the question order",
    );
    await total.getByLabel("Fixed number", { exact: true }).fill("0");
    assert((await preview.textContent()).includes("cannot divide by zero"));
    assert(
      await preview
        .getByRole("button", {
          name: "Check working form on server",
          exact: true,
        })
        .isDisabled(),
    );
    await total.getByLabel("Fixed number", { exact: true }).fill("3");

    await preview
      .getByRole("button", {
        name: "Check working form on server",
        exact: true,
      })
      .click();
    await preview
      .getByRole("status")
      .filter({ hasText: "Server validation passed" })
      .waitFor();
    assert((await preview.textContent()).includes("8.33"));
    assert.equal(
      await preview.getByLabel("Company", { exact: true }).count(),
      0,
    );
    await preview.getByLabel("Quantity", { exact: true }).fill("6");
    assert.equal(
      await preview
        .getByRole("status")
        .filter({ hasText: "Server validation passed" })
        .count(),
      0,
      "Changing input invalidates confirmation",
    );
    await preview
      .getByLabel("Company", { exact: true })
      .fill("Private preview company");
    await preview
      .getByRole("button", {
        name: "Check working form on server",
        exact: true,
      })
      .click();
    await preview
      .getByRole("status")
      .filter({ hasText: "Server validation passed" })
      .waitFor();
    await measure("working-preview");
    await page
      .getByRole("button", { name: "Publish form", exact: true })
      .click();
    await page
      .getByRole("status")
      .filter({ hasText: "Published. Visitors" })
      .waitFor();
    let saved = await (
      await owner.request.get(origin + `/api/admin/forms/${id}`)
    ).json();
    assert.equal(
      saved.definition.fields[2].calculation.expression.operation,
      "Round",
    );
    assert.equal(saved.definition.fields[3].visible_when.operation, "All");
    await page.goto(origin + `/admin/forms/${id}/workflows`);
    await page
      .getByLabel("Recipient", { exact: true })
      .fill("owner@example.test");
    await page
      .getByLabel("Subject", { exact: true })
      .fill("Qualified quotation");
    const routing = page.locator("#form-workflow-condition");
    await routing
      .getByLabel("Notify when an accepted field has a value", { exact: true })
      .selectOption({ label: "Total" });
    await routing.getByLabel("Condition", { exact: true }).selectOption("All");
    await routing
      .getByLabel("Rule type", { exact: true })
      .nth(0)
      .selectOption("Greater");
    await routing.getByLabel("Value to match", { exact: true }).fill("10");
    await routing
      .getByRole("button", { name: "Add rule", exact: true })
      .click();
    await routing
      .getByLabel("Rule field", { exact: true })
      .nth(1)
      .selectOption({ label: "Company" });
    await page
      .locator('[data-writing-canvas] [contenteditable="true"]')
      .fill("A canonical qualified response is ready for review.");
    await submit(
      page,
      page.getByRole("button", { name: "Add conditional route", exact: true }),
    );
    await page
      .getByRole("button", { name: "Restore recovery copy", exact: true })
      .click();
    assert.equal(
      await page
        .locator("#form-workflow-condition")
        .getByLabel("Condition", { exact: true })
        .inputValue(),
      "All",
      "Recovered hidden rules and visible composer agree",
    );
    assert.equal(
      await page
        .locator("#form-workflow-condition")
        .getByLabel("Value to match", { exact: true })
        .inputValue(),
      "10",
    );
    await measure("notification-routing");
    await page.evaluate(() => {
      const form = document
        .querySelector("#form-workflow-condition")
        .closest("form");
      const key = `wpalt:authoring:${form.dataset.owner}:${location.pathname}:main`;
      const saved = JSON.parse(localStorage.getItem(key));
      saved.fields.condition_json = JSON.stringify({
        operation: "Unknown",
        field: "quantity",
      });
      localStorage.setItem(key, JSON.stringify(saved));
    });
    await page.reload();
    await page
      .getByRole("button", { name: "Restore recovery copy", exact: true })
      .click();
    await page
      .getByRole("alert")
      .filter({ hasText: "recovered notification rule needs review" })
      .waitFor();
    await page.waitForFunction(
      () => document.querySelector("[data-add-notification]").disabled,
    );
    await page
      .locator("#form-workflow-condition")
      .getByLabel("Notify when an accepted field has a value", { exact: true })
      .selectOption({ label: "Total" });
    await page.waitForFunction(
      () => !document.querySelector("[data-add-notification]").disabled,
    );

    await page.goto(origin + `/admin/forms/${id}`);
    await page
      .getByRole("button", { name: "Publish form", exact: true })
      .click();
    await page
      .getByRole("status")
      .filter({ hasText: "Published. Visitors" })
      .waitFor();
    saved = await (
      await owner.request.get(origin + `/api/admin/forms/${id}`)
    ).json();
    assert.equal(saved.definition.notifications[0].when.operation, "All");
    const emptyReport = await (
      await owner.request.get(
        origin +
          `/api/admin/forms/${id}/report?version=${saved.published_version}`,
      )
    ).json();
    assert.equal(
      emptyReport.responses,
      0,
      "Working previews never create responses",
    );
    if (process.env.WPALT_FORMINATOR_REFERENCE)
      await require("./forminator_reference.cjs")(
        owner,
        origin,
        output,
        id,
        csrf,
      );
    await page.goto(origin + `/forms/${id}`);
    await page.getByLabel("Quantity", { exact: true }).fill("2");
    await page.getByLabel("Rate", { exact: true }).fill("12.5");
    assert.equal(await page.getByLabel("Company", { exact: true }).count(), 0);
    assert.equal(await page.locator("output").textContent(), "8.33");
    await measure("public-calculator", false);
    await page
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await page
      .getByRole("status")
      .filter({ hasText: "Your response has been received" })
      .waitFor();
    const native = await owner.browser().newContext({
      javaScriptEnabled: false,
      viewport: { width: 320, height: 1000 },
    });
    try {
      const p = await native.newPage();
      await p.goto(origin + `/forms/${id}/native`);
      await p.getByLabel("Quantity", { exact: true }).fill("6");
      await p.getByLabel("Rate", { exact: true }).fill("12.5");
      await submit(
        p,
        p.getByRole("button", {
          name: "Review calculations and fields",
          exact: true,
        }),
      );
      assert.equal(await p.locator("output").textContent(), "25.0");
      await p
        .getByLabel("Company", { exact: true })
        .fill("Private native company");
      await submit(
        p,
        p.getByRole("button", { name: "Send response", exact: true }),
      );
      assert(
        (await p.locator("main").textContent()).includes(
          "Your response has been received",
        ),
      );
    } finally {
      await native.close();
    }
    await page.goto(origin + `/admin/forms/${id}/report`);
    assert(
      !(await page.locator("main").textContent()).includes(
        "Private native company",
      ),
    );
    assert(
      !(await page.locator("main").textContent()).includes(
        "Private preview company",
      ),
    );
    await measure("versioned-report");
    let result = await (
      await owner.request.get(
        origin +
          `/api/admin/forms/${id}/report?version=${saved.published_version}`,
      )
    ).json();
    assert.equal(result.responses, 2);
    assert.deepEqual(
      [
        result.fields[3].eligible,
        result.fields[3].answered,
        result.fields[3].unanswered,
        result.fields[3].skipped,
      ],
      [1, 1, 0, 1],
    );
    const exported = await owner.request.get(
      origin +
        `/admin/forms/${id}/report.csv?version=${saved.published_version}`,
    );
    assert(exported.ok());
    assert.equal(exported.headers()["cache-control"], "no-store");
    assert(!(await exported.text()).includes("Private native company"));
    const invalid = await owner.request.post(
      origin + `/api/admin/forms/${id}/preview`,
      {
        headers: { Origin: origin },
        data: { csrf: "wrong", definition: saved.definition, values: {} },
      },
    );
    assert.equal(invalid.status(), 403);
    assert.equal(
      (
        await visitors.request.get(origin + `/api/admin/forms/${id}/report`, {
          maxRedirects: 0,
        })
      ).status(),
      401,
    );
    report.assertions.push(
      "Visual recursive calculator and all-rules composer save actual typed structure; private working preview is invalidated by edits and records no response",
      "Actual public calculator and no-JavaScript canonical submission share visibility and arithmetic; reports exclude private response text and distinguish skipped answers",
      "Private versioned HTML/JSON/CSV require permission; invalid CSRF cannot invoke working preview",
    );
    assert.deepEqual(report.errors, []);
    report.status = "passed";
  } catch (error) {
    report.errors.push(error.message);
    report.status = "failed";
    throw error;
  } finally {
    fs.writeFileSync(
      path.join(output, "d10-advanced-forms.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
    await page.close();
  }
};
