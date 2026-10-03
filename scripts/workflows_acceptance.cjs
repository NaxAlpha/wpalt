// A complete contribution, recovery, mailbox-proof and approval journey.
const assert = require("node:assert/strict"),
  fs = require("node:fs"),
  path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async function (owner, origin, output) {
  const page = await owner.newPage(),
    context = await owner
      .browser()
      .newContext({ viewport: { width: 320, height: 900 } }),
    visitor = await context.newPage();
  const report = {
    journey: [],
    script_errors: [],
    accessibility: [],
    measurements: [],
  };
  for (const p of [page, visitor]) {
    p.on("pageerror", (e) => report.script_errors.push(e.message));
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
    await page.goto(origin + "/admin/audience");
    await page.getByLabel("Title", { exact: true }).fill("Release notes");
    await page
      .getByLabel("Subscription purpose", { exact: true })
      .fill("Send local release notes");
    await submit(
      page.getByRole("button", { name: "Create list", exact: true }),
    );
    await page.goto(origin + "/admin/forms");
    await page
      .getByLabel("Title", { exact: true })
      .fill("Community contribution");
    await submit(
      page.getByRole("button", { name: "Create form", exact: true }),
    );
    const id = new URL(page.url()).pathname.split("/").at(-1);
    const csrf = await page.locator("#forms-studio").getAttribute("data-csrf");
    const initial = await (
      await owner.request.get(origin + `/api/admin/forms/${id}`)
    ).json();
    const list = initial.lists.find((l) => l.title === "Release notes");
    assert.ok(list);
    const field = (name, label, extra = {}) => ({
      name,
      schema: { kind: "string", label, required: true },
      ...extra,
    });
    const definition = {
      title: "Community contribution",
      fields: [
        field("name", "Your full name"),
        field("email", "Your email", { widget: { kind: "Email" } }),
        field("title", "Contribution title"),
        field("message", "Contribution text", { widget: { kind: "TextArea" } }),
        {
          name: "attachment",
          schema: { kind: "string", label: "Supporting file" },
          widget: { kind: "Upload" },
        },
        {
          name: "consent",
          schema: { kind: "boolean", label: "Receive release notes" },
        },
      ],
      subscription: {
        list: list.id,
        policy: list.policy,
        email_field: "email",
        consent_field: "consent",
      },
      registration: { email_field: "email", name_field: "name" },
      draft_post: { title_field: "title", body_field: "message" },
      notifications: [
        {
          recipient: "editor@example.test",
          subject: "Review community contribution",
          document: {
            version: 1,
            root: {
              type: "doc",
              content: [
                {
                  type: "paragraph",
                  content: [
                    {
                      type: "text",
                      text: "Review the private response in Forms.",
                    },
                  ],
                },
              ],
            },
          },
        },
      ],
    };
    definition.notifications[0].document = JSON.stringify(
      definition.notifications[0].document,
    );
    const saved = await owner.request.post(origin + `/api/admin/forms/${id}`, {
      data: { csrf, version: initial.version, definition, publish: true },
      headers: { Origin: origin },
    });
    assert.equal(saved.status(), 200, await saved.text());
    await page.goto(origin + `/admin/forms/${id}/workflows`);
    await ui.accessibility(page, origin, "frozen-form-workflows", report);
    await page.screenshot({
      path: path.join(output, "form-workflows.png"),
      fullPage: true,
    });
    await page.goto(origin + "/admin/campaigns");
    await page.getByLabel("Title", { exact: true }).fill("Confirmed welcome");
    await page
      .getByRole("combobox", { name: /^Audience list/ })
      .selectOption(list.id);
    await submit(
      page.getByRole("button", { name: "Create campaign", exact: true }),
    );
    await page
      .getByLabel("Subject", { exact: true })
      .fill("Welcome to release notes");
    await page.locator(".ProseMirror").click();
    await page.keyboard.type(
      "Thanks for confirming. You control your subscription.",
    );
    await page
      .getByLabel("Send automatically when a subscriber confirms this list", {
        exact: true,
      })
      .check();
    await submit(
      page.getByRole("button", { name: "Save campaign", exact: true }),
    );
    assert.ok(await page.getByLabel("Send automatically when a subscriber confirms this list", { exact: true }).isChecked(), "Saved confirmation trigger remains enabled.");
    await ui.accessibility(page, origin, "triggered-campaign", report);
    await visitor.goto(origin + `/forms/${id}`);
    const publicDefinition = await visitor
      .locator("#public-form")
      .getAttribute("data-definition");
    assert.ok(
      !publicDefinition.includes("editor@example.test"),
      "Public field grammar cannot expose private routing recipients.",
    );
    assert.equal(JSON.parse(publicDefinition).form.registration, null);
    await visitor
      .getByLabel("Your full name", { exact: true })
      .fill("Local applicant");
    await visitor
      .getByLabel("Your email", { exact: true })
      .fill("applicant-browser@example.test");
    await visitor
      .getByLabel("Contribution title", { exact: true })
      .fill("A private contribution draft");
    await visitor
      .getByLabel("Contribution text", { exact: true })
      .fill("PRIVATE_CONTRIBUTION_SENTINEL");
    await visitor.getByLabel("Supporting file", { exact: true }).setInputFiles({
      name: "参考.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("PRIVATE_ATTACHMENT_SENTINEL"),
    });
    await visitor
      .getByText("Private attachment ready.", { exact: false })
      .waitFor();
    await visitor.getByLabel("Receive release notes", { exact: true }).check();
    await visitor.getByText("Save and resume", { exact: true }).click();
    await visitor
      .getByLabel("Save this draft on this device for seven days", {
        exact: true,
      })
      .check();
    await visitor
      .getByRole("button", { name: "Save private server draft", exact: true })
      .click();
    const notice = visitor
      .getByRole("status")
      .filter({ hasText: "Server draft saved" });
    await notice.waitFor();
    const token = (await notice.innerText()).match(/#draft=([a-f0-9]{64})/)[1];
    await visitor.goto(origin + `/forms/${id}#draft=${token}`);
    await visitor
      .getByRole("status")
      .filter({ hasText: "Server draft restored" })
      .waitFor();
    assert.equal(
      await visitor
        .getByLabel("Contribution text", { exact: true })
        .inputValue(),
      "PRIVATE_CONTRIBUTION_SENTINEL",
    );
    assert.equal(
      new URL(visitor.url()).hash,
      "",
      "Recovery capability leaves the address bar before any subsequent navigation.",
    );
    await context.setOffline(true);
    await visitor
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await visitor
      .locator(".notice.error,.ui-notice-error,[role=alert]")
      .first()
      .waitFor();
    await context.setOffline(false);
    await visitor.reload();
    await visitor
      .getByRole("status")
      .filter({ hasText: "device draft is available" })
      .waitFor();
    assert.ok(
      await visitor.getByLabel("Your full name", { exact: true }).isDisabled(),
      "Uncertain submission keeps its original payload frozen across reload.",
    );
    await visitor
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await visitor
      .getByRole("status")
      .filter({ hasText: "response has been received" })
      .waitFor();
    assert.equal(
      await visitor.evaluate(
        (id) => localStorage.getItem(`wpalt:form:${id}`),
        id,
      ),
      null,
      "Accepted response clears the opted-in device copy.",
    );
    await page.goto(origin + "/admin/mail");
    const proofJob = await page
      .getByRole("link", { name: "Verify your account request", exact: true })
      .getAttribute("href");
    const proof = (
      await (await owner.request.get(origin + proofJob + "/download")).text()
    ).replace(/=\r?\n/g, "");
    const registration = proof.match(/\/registration\/([a-f0-9]{64})/)[1];
    const confirmationJob = await page
      .getByRole("link", { name: "Confirm your subscription", exact: true })
      .getAttribute("href");
    const confirmation = (
      await (
        await owner.request.get(origin + confirmationJob + "/download")
      ).text()
    ).replace(/=\r?\n/g, "");
    const confirm = confirmation.match(
      /\/audience\/confirm\/([a-f0-9]{64})/,
    )[1];
    await visitor.goto(origin + `/audience/confirm/${confirm}`);
    await Promise.all([
      visitor.waitForNavigation({ waitUntil: "load" }),
      visitor.getByRole("button", { name: /Confirm/i }).click(),
    ]);
    assert.ok((await visitor.locator("body").innerText()).includes("confirmed"), "Confirmation completed before the owner inspects its triggered queue.");
    await page.goto(origin + "/admin/mail");
    await page
      .getByRole("link", { name: "Welcome to release notes", exact: true })
      .waitFor();
    await visitor.goto(origin + `/registration/${registration}`);
    await visitor
      .getByLabel("New password", { exact: true })
      .fill("browser applicant password");
    await Promise.all([
      visitor.waitForNavigation(),
      visitor
        .getByRole("button", { name: "Verify account request", exact: true })
        .click(),
    ]);
    await visitor
      .getByRole("heading", { name: "Mailbox verified", exact: true })
      .waitFor();
    await page.goto(origin + "/admin/registrations");
    await submit(
      page.getByRole("button", { name: "Approve subscriber", exact: true }),
    );
    await visitor.goto(origin + "/login");
    await visitor
      .locator("input[name=email]")
      .fill("applicant-browser@example.test");
    await visitor
      .locator("input[name=password]")
      .fill("browser applicant password");
    await Promise.all([
      visitor.waitForURL("**/account"),
      visitor.getByRole("button", { name: "Sign in", exact: true }).click(),
    ]);
    assert.equal(
      (await context.request.get(origin + "/api/admin/forms/" + id)).status(),
      403,
      "Approved subscribers still cannot read private form administration.",
    );
    // Insert the same published form through the real writing editor.
    await page.goto(origin + "/admin/posts/new");
    await page.getByLabel("Title", { exact: true }).fill("Participate locally");
    await page.getByLabel("Slug", { exact: true }).fill("participate-locally");
    await page.getByRole("button", { name: "Blocks", exact: true }).click();
    await page
      .getByRole("button", { name: "Published form", exact: true })
      .click();
    const dialog = page.getByRole("dialog", {
      name: "Insert published form",
      exact: true,
    });
    await dialog.getByLabel("Published form", { exact: true }).selectOption(id);
    await dialog
      .getByRole("button", { name: "Insert form", exact: true })
      .click();
    await submit(
      page.getByRole("button", { name: "Publish now", exact: true }),
    );
    await visitor.goto(origin + "/participate-locally");
    const embedded = visitor.frameLocator(".form-embed iframe");
    await embedded.getByLabel("Your full name", { exact: true }).waitFor();
    assert.equal(
      await embedded.locator(".site-header").count(),
      0,
      "Embedded forms keep one integrated site navigation.",
    );
    assert.equal(
      await embedded.locator("#engagement-controls").count(),
      0,
      "Embedded forms cannot duplicate privacy prompts or analytics capture.",
    );
    await visitor.screenshot({
      path: path.join(output, "embedded-form-320.png"),
      fullPage: true,
    });
    report.journey.push(
      "Protected UTF-8 file upload, expiring server recovery, device opt-in and offline retry preserve one response; consent confirmation triggers mail; mailbox proof and approval produce a subscriber; the writing editor embeds a published form.",
    );
    assert.deepEqual(report.script_errors, []);
    fs.writeFileSync(
      path.join(output, "business-workflows.json"),
      JSON.stringify(report, null, 2) + "\n",
    );
  } catch (error) {
    await page.screenshot({
      path: path.join(output, "workflow-owner-failure.png"),
      fullPage: true,
    });
    await visitor.screenshot({
      path: path.join(output, "workflow-visitor-failure.png"),
      fullPage: true,
    });
    throw error;
  } finally {
    await page.close();
    await context.close();
  }
};
