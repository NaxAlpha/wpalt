// A connected owner/learner journey, including direct-resource enforcement and reflow.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, origin, output) => {
  const page = await owner.newPage();
  const context = await owner
    .browser()
    .newContext({ viewport: { width: 1440, height: 1000 } });
  const learner = await context.newPage();
  const report = {
    journey: [],
    measurements: [],
    accessibility: [],
    script_errors: [],
    remote_requests: [],
  };
  const submit = async (p, button) =>
    Promise.all([p.waitForNavigation({ waitUntil: "load" }), button.click()]);
  const section = (p, title) =>
    p.getByRole("heading", { name: title, exact: true }).locator("..");
  for (const p of [page, learner]) {
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
    await p.route(origin + "/__ui_fixture/member-spacing.css", (r) =>
      r.fulfill({
        contentType: "text/css",
        body: "body * {line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important} p {margin-bottom:2em!important}",
      }),
    );
  }
  const measure = async (p, name) => {
    await p.waitForLoadState("load");
    await p.evaluate(() => document.fonts.ready);
    for (const width of [320, 768, 1440]) {
      await p.setViewportSize({ width, height: 1000 });
      const geometry = await ui.geometry(p);
      report.measurements.push({ surface: name, ...geometry });
      assert.deepEqual(geometry.failures, [], `${name} at ${width}`);
      await ui.accessibility(p, origin, name, report);
      const labels = await p
        .locator('label:has(input[type="radio"])')
        .evaluateAll((nodes) =>
          nodes.map((n) => ({
            width: n.getBoundingClientRect().width,
            height: n.getBoundingClientRect().height,
          })),
        );
      assert(
        labels.every((n) => n.width >= 44 && n.height >= 44),
        "Quiz labels retain a 44px hit area",
      );
      await p.screenshot({
        path: path.join(output, `membership-${name}-${width}.png`),
        fullPage: true,
      });
      const spacing = await p.addStyleTag({
        url: origin + "/__ui_fixture/member-spacing.css",
      });
      const spaced = await ui.geometry(p);
      report.measurements.push({
        surface: name,
        state: "text-spacing",
        ...spaced,
      });
      assert.deepEqual(
        spaced.failures,
        [],
        `${name} with user text spacing at ${width}`,
      );
      await spacing.evaluate((n) => n.remove());
    }
  };
  try {
    const password = crypto.randomBytes(20).toString("hex");
    await page.goto(origin + "/admin/users");
    const account = page.locator("form").filter({
      has: page.getByRole("button", { name: "Create account", exact: true }),
    });
    await account.getByLabel("Name", { exact: true }).fill("M5 Learner");
    await account
      .getByLabel("Email", { exact: true })
      .fill("m5-learner@example.test");
    await account
      .getByLabel("Role", { exact: true })
      .selectOption("subscriber");
    await account
      .getByLabel("Initial password", { exact: true })
      .fill(password);
    await submit(
      page,
      account.getByRole("button", { name: "Create account", exact: true }),
    );
    for (const [title, slug] of [
      ["M5 foundations", "m5-foundations"],
      ["M5 project", "m5-project"],
    ]) {
      await page.goto(origin + "/admin/posts/new");
      await page.getByLabel("Title", { exact: true }).fill(title);
      await page.getByRole("textbox", { name: /^URL slug/ }).fill(slug);
      await page.locator(".ProseMirror").fill("PRIVATE_LEARNING_BODY_" + slug);
      await submit(
        page,
        page.getByRole("button", { name: "Publish now", exact: true }),
      );
    }
    await page.goto(origin + "/admin/media");
    const uploadForm = page.locator('form[enctype="multipart/form-data"]');
    await uploadForm
      .getByLabel("Image", { exact: true })
      .setInputFiles(path.join(__dirname, "../tests/fixtures/green.png"));
    await uploadForm
      .getByLabel("Alternative text", { exact: true })
      .fill("M5 course worksheet");
    await uploadForm
      .getByLabel("Visibility", { exact: true })
      .selectOption("private");
    await submit(
      page,
      page.getByRole("button", { name: "Upload image", exact: true }),
    );
    const mediaUrl = await page
      .locator('img[alt="M5 course worksheet"]')
      .getAttribute("src");
    await page.goto(origin + "/admin/members");
    let form = section(page, "Create an access policy");
    await form
      .getByLabel("Policy title", { exact: true })
      .fill("M5 academy access");
    await form
      .getByLabel("Entitlement key", { exact: true })
      .fill("m5-academy");
    await submit(
      page,
      form.getByRole("button", { name: "Create policy", exact: true }),
    );
    form = section(page, "Assign a membership");
    await form
      .locator("select[name=user]")
      .selectOption({ label: "M5 Learner · m5-learner@example.test" });
    await form
      .getByLabel("Entitlement key", { exact: true })
      .fill("m5-academy");
    await submit(
      page,
      form.getByRole("button", { name: "Assign access", exact: true }),
    );
    await measure(page, "owner-members");
    await page.goto(origin + "/admin/courses");
    await page
      .getByLabel("Course title", { exact: true })
      .fill("M5 local academy");
    await page
      .locator("select[name=policy]")
      .selectOption({ label: "M5 academy access" });
    await page
      .locator("select[name=post]")
      .selectOption({ label: "M5 foundations" });
    await submit(
      page,
      page.getByRole("button", { name: "Create course", exact: true }),
    );
    const courseUrl = page.url(),
      id = courseUrl.split("/").pop();
    form = section(page, "1. First lesson");
    await form.getByLabel("Lesson title", { exact: true }).fill("Foundations");
    await form
      .getByLabel("Quiz prompt (leave blank for no quiz)", { exact: true })
      .fill("Where is authority stored?");
    await form
      .getByLabel("Choices, one per line", { exact: true })
      .fill("On this server\nIn a vendor account");
    await submit(
      page,
      form.getByRole("button", { name: "Save lesson draft", exact: true }),
    );
    form = section(page, "Quiz questions · Foundations");
    await form.getByText("Add a quiz question", { exact: true }).click();
    await form
      .getByLabel("Question prompt", { exact: true })
      .fill("What belongs in debug logs?");
    await form
      .getByLabel("Answer choices, one per line", { exact: true })
      .fill("Performance timings\nSecrets");
    await submit(
      page,
      form.getByRole("button", { name: "Add question", exact: true }),
    );
    form = section(page, "Add a lesson");
    await form.getByLabel("Lesson title", { exact: true }).fill("Project");
    await form
      .locator("select[name=post]")
      .selectOption({ label: "M5 project" });
    await submit(
      page,
      form.getByRole("button", { name: "Add lesson", exact: true }),
    );
    form = section(page, "2. Project");
    await form
      .getByLabel("Assignment instructions (optional)", { exact: true })
      .fill("Describe your local project");
    await submit(
      page,
      form.getByRole("button", { name: "Save lesson draft", exact: true }),
    );
    form = section(page, "Protected downloads · Project");
    await form
      .locator("select[name=media]")
      .selectOption(
        await form
          .locator("option")
          .filter({ hasText: "M5 course worksheet" })
          .getAttribute("value"),
      );
    await submit(
      page,
      form.getByRole("button", {
        name: "Assign protected download",
        exact: true,
      }),
    );
    await submit(
      page,
      page.getByRole("button", { name: "Publish course edition", exact: true }),
    );
    await measure(page, "course-composition");
    await learner.goto(origin + "/login");
    await learner
      .getByLabel("Email", { exact: true })
      .fill("m5-learner@example.test");
    await learner.getByLabel("Password", { exact: true }).fill(password);
    await submit(
      learner,
      learner.getByRole("button", { name: "Sign in", exact: true }),
    );
    await learner.goto(origin + "/members");
    await measure(learner, "member-home");
    assert.equal(
      (await context.request.get(origin + "/m5-project")).status(),
      403,
    );
    assert.equal(
      (await context.request.get(new URL(mediaUrl, origin).href)).status(),
      403,
    );
    await learner.goto(origin + "/members/courses/" + id);
    await measure(learner, "course-progress");
    await learner
      .getByRole("link", { name: "Foundations", exact: true })
      .click();
    const firstLessonUrl = learner.url();
    await measure(learner, "quiz");
    for (const choice of ["In a vendor account", "Secrets"]) {
      await learner.getByLabel(choice, { exact: true }).focus();
      await learner.keyboard.press("Space");
      assert(await learner.getByLabel(choice, { exact: true }).isChecked());
    }
    await submit(
      learner,
      learner.getByRole("button", { name: "Submit assessment", exact: true }),
    );
    assert((await learner.innerText("body")).includes("Score: 0%"));
    await learner.goto(firstLessonUrl);
    for (const choice of ["On this server", "Performance timings"]) {
      await learner.getByLabel(choice, { exact: true }).focus();
      await learner.keyboard.press("Space");
      assert(await learner.getByLabel(choice, { exact: true }).isChecked());
    }
    await submit(
      learner,
      learner.getByRole("button", { name: "Submit assessment", exact: true }),
    );
    assert((await learner.innerText("body")).includes("Lesson complete"));
    await learner.goto(origin + "/members/courses/" + id);
    await learner.getByRole("link", { name: "Project", exact: true }).click();
    await learner
      .getByLabel("Describe your local project", { exact: true })
      .fill("My local site keeps access rules and data under owner control.");
    assert.equal(
      (await context.request.get(new URL(mediaUrl, origin).href)).status(),
      200,
    );
    await measure(learner, "assignment");
    await submit(
      learner,
      learner.getByRole("button", { name: "Submit assessment", exact: true }),
    );
    assert(
      (await learner.innerText("body")).includes("Awaiting assignment review"),
    );
    await page.goto(origin + "/admin/members");
    const review = page
      .locator("article")
      .filter({ hasText: "My local site keeps access rules" });
    await review
      .getByLabel("Feedback", { exact: true })
      .fill("Approved: clear ownership and delivery boundary.");
    await submit(
      page,
      review.getByRole("button", { name: "Approve work", exact: true }),
    );
    await learner.goto(origin + "/members/courses/" + id);
    await submit(
      learner,
      learner.getByRole("button", { name: /certificate/i }),
    );
    await measure(learner, "certificate");
    await learner.goto(origin + "/members/profile");
    await learner
      .getByLabel("Biography", { exact: true })
      .fill("An independent learner on a local site.");
    await submit(
      learner,
      learner.getByRole("button", { name: "Save profile", exact: true }),
    );
    await measure(learner, "profile");
    await page.goto(origin + "/admin/members");
    form = section(page, "Create a group or organization");
    await form
      .getByLabel("Group title", { exact: true })
      .fill("M5 learning team");
    await form
      .locator("select[name=user]")
      .selectOption({ label: "M5 Learner" });
    await form
      .getByLabel("Seat limit (0 means ordinary group)", { exact: true })
      .fill("2");
    await submit(
      page,
      form.getByRole("button", { name: "Create group", exact: true }),
    );
    const groupPath = await page
      .getByRole("link", { name: "M5 learning team", exact: true })
      .getAttribute("href");
    await learner.goto(origin + groupPath);
    await learner
      .getByLabel("Existing member email", { exact: true })
      .fill("m5-learner@example.test");
    await submit(
      learner,
      learner.getByRole("button", { name: "Add seat", exact: true }),
    );
    await learner
      .getByLabel("Message", { exact: true })
      .fill("Our local learning discussion awaits moderation.");
    await submit(
      learner,
      learner.getByRole("button", { name: "Submit for review", exact: true }),
    );
    await measure(learner, "community");
    await page.goto(origin + "/admin/members");
    const discussion = section(page, "Discussion moderation")
      .locator("article")
      .filter({ hasText: "Our local learning discussion" });
    await submit(
      page,
      discussion.getByRole("button", { name: "Approve", exact: true }),
    );
    await learner.goto(origin + groupPath);
    await learner.getByText("approved", { exact: true }).waitFor();
    await page.goto(origin + "/admin/members");
    form = section(page, "Create a single-use gift");
    await form.getByLabel("Entitlement key", { exact: true }).fill("m5-gift");
    await submit(
      page,
      form.getByRole("button", { name: "Create gift link", exact: true }),
    );
    const giftPath = await page
      .locator('a[href^="/members/gifts/"]')
      .getAttribute("href");
    await learner.goto(origin + giftPath);
    await measure(learner, "gift");
    await submit(
      learner,
      learner.getByRole("button", { name: "Claim gift", exact: true }),
    );
    assert.equal(
      learner.url(),
      origin + "/members",
      "Gift claim should complete with a member-dashboard redirect",
    );
    assert.equal(
      (
        await context.request.get(origin + giftPath, { maxRedirects: 0 })
      ).status(),
      404,
    );
    await page.goto(origin + "/admin/members/referrals");
    form = section(page, "Create a referral");
    await form.getByLabel("Title", { exact: true }).fill("M5 local referral");
    await form
      .locator("select[name=user]")
      .selectOption({ label: "M5 Learner" });
    await submit(
      page,
      form.getByRole("button", { name: "Create referral link", exact: true }),
    );
    const referralPath = await page
      .getByRole("link", { name: "M5 local referral", exact: true })
      .getAttribute("href");
    assert.equal(
      (await context.request.get(origin + referralPath)).status(),
      200,
    );
    await page.reload();
    form = section(page, "Record a commission");
    await form
      .locator("select[name=id]")
      .selectOption({ label: "M5 local referral" });
    await form
      .getByLabel("Unique reference", { exact: true })
      .fill("M5-manual-obligation");
    await form
      .getByLabel("Amount in minor units", { exact: true })
      .fill("1200");
    await form.getByLabel("Currency code", { exact: true }).fill("USD");
    await submit(
      page,
      form.getByRole("button", { name: "Record commission", exact: true }),
    );
    await measure(page, "referrals");
    await page.goto(origin + "/admin/members/identity");
    await measure(page, "identity-disabled");
    report.journey.push(
      "Private profile, delegated group seats, moderated community, one-use gift, local referral visits and manual commission administration.",
    );
    await page.goto(courseUrl);
    await measure(page, "gradebook");
    await page.goto(origin + "/admin/members");
    const grant = section(page, "Assign a membership")
      .locator("div.toolbar")
      .filter({ hasText: "M5 Learner" })
      .filter({ hasText: "m5-academy" });
    await submit(
      page,
      grant.getByRole("button", { name: "Revoke", exact: true }),
    );
    assert.equal(
      (await context.request.get(origin + "/members/courses/" + id)).status(),
      403,
    );
    assert.equal(
      (await context.request.get(new URL(mediaUrl, origin).href)).status(),
      403,
    );
    report.journey.push(
      "Owner creates a member, shared lessons, policy, grant, ordered course, quiz and protected download.",
      "A wrong attempt preserves prerequisite protection; passing unlocks the next lesson and its download.",
      "Assignment approval completes the course and produces a local certificate.",
      "Revocation blocks course and direct download access immediately.",
    );
    assert.deepEqual(report.script_errors, []);
    assert.deepEqual(report.remote_requests, []);
  } catch (error) {
    console.error(
      "Membership owner URL:",
      page.url(),
      "Body:",
      await page.locator("body").innerText(),
    );
    await page.screenshot({
      path: path.join(output, "membership-failure.png"),
      fullPage: true,
    });
    throw error;
  } finally {
    fs.writeFileSync(
      path.join(output, "membership-results.json"),
      JSON.stringify(report, null, 2),
    );
    await page.close();
    await context.close();
  }
};
