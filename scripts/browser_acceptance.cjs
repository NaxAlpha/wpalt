// Reviewable real-browser workflow. The app needs no Node runtime; this tester does.
const fs = require("node:fs");
const path = require("node:path");
const net = require("node:net");
const crypto = require("node:crypto");
const { spawn, spawnSync } = require("node:child_process");
const assert = require("node:assert/strict");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const root = path.resolve(__dirname, "..");
const output = path.join(root, "work/browser-evidence");
fs.mkdirSync(output, { recursive: true });
const temporary = fs.mkdtempSync(path.join(root, "work/browser-site-"));
const binary = path.resolve(
  process.env.WPALT_BINARY || path.join(root, "target/debug/wpalt"),
);
const password = crypto.randomBytes(24).toString("hex");
let server, browser, logFd;
function command(config, args, input) {
  const r = spawnSync(binary, ["--config", config, ...args], {
    input,
    encoding: "utf8",
  });
  assert.equal(r.status, 0, r.stderr);
}
async function submit(page, button) {
  await Promise.all([
    page.waitForNavigation({ waitUntil: "load" }),
    button.click(),
  ]);
}
async function freePort() {
  const s = net.createServer();
  await new Promise((resolve) => s.listen(0, "127.0.0.1", resolve));
  const p = s.address().port;
  await new Promise((resolve) => s.close(resolve));
  return p;
}
(async () => {
  const port = await freePort(),
    origin = `http://127.0.0.1:${port}`;
  const config = path.join(temporary, "site.toml");
  fs.writeFileSync(
    config,
    `database_url = "sqlite://${temporary}/site.db?mode=rwc"\ndata_dir = "${temporary}/data"\nlisten = "127.0.0.1:${port}"\nbase_url = "${origin}"\n`,
  );
  command(
    config,
    ["init", "--admin-email", "owner@example.test"],
    password + "\n",
  );
  command(config, ["seed-demo"]);
  logFd = fs.openSync(path.join(temporary, "server.log"), "w");
  server = spawn(binary, ["--config", config, "serve"], {
    stdio: ["ignore", logFd, logFd],
  });
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(origin + "/health")).ok) break;
    } catch (_) {}
    assert.equal(server.exitCode, null, "Server failed to start");
    if (i === 99) throw Error("Server readiness timed out");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROME_EXECUTABLE
      ? { executablePath: process.env.CHROME_EXECUTABLE }
      : {}),
  });
  const owner = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  });
  const publicContext = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
  });
  const page = await owner.newPage(),
    visitor = await publicContext.newPage();
  const errors = [],
    remote = [];
  for (const context of [owner, publicContext]) {
    context.on("page", (p) => p.on("pageerror", (e) => errors.push(e.message)));
    context.on("request", (r) => {
      if (!r.url().startsWith(origin)) remote.push(r.url());
    });
  }
  page.on("pageerror", (e) => errors.push(e.message));
  visitor.on("pageerror", (e) => errors.push(e.message));
  await visitor.goto(origin);
  await visitor
    .getByRole("heading", { name: "The Local Journal", exact: true })
    .waitFor();
  await visitor.screenshot({
    path: path.join(output, "public-desktop.png"),
    fullPage: true,
  });
  await page.goto(origin + "/login");
  await page.getByLabel("Email", { exact: true }).fill("owner@example.test");
  await page.getByLabel("Password", { exact: true }).fill(password);
  await submit(
    page,
    page.getByRole("button", { name: "Sign in", exact: true }),
  );
  await page.waitForURL(origin + "/admin");
  if (!process.env.WPALT_SKIP_UI)
    await require("./ui_contracts.cjs")(owner, origin);
  await page.screenshot({
    path: path.join(output, "admin-desktop.png"),
    fullPage: true,
  });
  await page.getByRole("link", { name: "Create content", exact: true }).click();
  await page
    .getByLabel("Title", { exact: true })
    .fill("A browser-tested story");
  await page.getByRole("textbox", { name: /^URL slug/ }).fill("browser-story");
  await page
    .locator(".ProseMirror")
    .fill(
      "BROWSER_WORKING_DRAFT\n\nA useful story from our independent website.",
    );
  await page.getByLabel("Categories", { exact: true }).fill("Browser journeys");
  await submit(
    page,
    page.getByRole("button", { name: "Save draft", exact: true }),
  );
  await page.waitForURL(/\/admin\/posts\/[a-f0-9-]+$/);
  const editorUrl = page.url();

  assert.equal(
    (await publicContext.request.get(origin + "/browser-story")).status(),
    404,
  );
  await submit(
    page,
    page.getByRole("button", { name: "Publish now", exact: true }),
  );
  await page.waitForURL(editorUrl);
  await visitor.goto(origin + "/browser-story");

  await visitor.getByText("BROWSER_WORKING_DRAFT", { exact: false }).waitFor();
  await page.locator(".ProseMirror").fill("BROWSER_PRIVATE_AUTOSAVE");
  await page
    .getByText("Draft saved · live page unchanged", { exact: true })
    .waitFor({ timeout: 15000 });
  await visitor.reload();
  assert(!(await visitor.content()).includes("BROWSER_PRIVATE_AUTOSAVE"));
  const revisions = page.locator('form[action*="/revisions/"]');
  await submit(
    page,
    revisions.last().getByRole("button", { name: "Restore working copy" }),
  );
  await page.waitForURL(editorUrl);
  assert(
    (await page.locator(".ProseMirror").innerText()).includes(
      "BROWSER_WORKING_DRAFT",
    ),
  );
  await page.screenshot({
    path: path.join(output, "editor-desktop.png"),
    fullPage: true,
  });
  await page.goto(origin + "/admin/media");
  await page
    .getByLabel("Image", { exact: true })
    .setInputFiles(path.join(root, "tests/fixtures/green.png"));
  await page.getByLabel("Visibility", { exact: true }).selectOption("private");
  await page
    .getByLabel("Alternative text", { exact: true })
    .fill("A green example image");
  await submit(
    page,
    page.getByRole("button", { name: "Upload image", exact: true }),
  );
  const card = page.locator(".media-card").first();
  await card.waitFor();
  const mediaUrl = await card.locator("img").getAttribute("src");
  assert.equal(
    (await publicContext.request.get(origin + mediaUrl)).status(),
    401,
  );
  await card.getByLabel("Visibility", { exact: true }).selectOption("public");
  await submit(
    page,
    card.getByRole("button", { name: "Save details", exact: true }),
  );
  await page.waitForURL(origin + "/admin/media");
  assert.equal(
    (await publicContext.request.get(origin + mediaUrl)).status(),
    200,
  );
  await visitor.goto(origin + "/browser-story");
  await visitor.getByLabel("Your name", { exact: true }).fill("A reader");
  await visitor
    .getByLabel("Comment", { exact: true })
    .fill("BROWSER_COMMENT_TO_REVIEW");
  await submit(
    visitor,
    visitor.getByRole("button", { name: "Submit for review", exact: true }),
  );
  await visitor
    .getByRole("heading", { name: "Thank you for joining in.", exact: true })
    .waitFor();
  await page.goto(origin + "/admin/comments");
  await submit(
    page,
    page.getByRole("button", { name: "Approve", exact: true }),
  );
  await visitor.goto(origin + "/browser-story");
  await visitor
    .getByText("BROWSER_COMMENT_TO_REVIEW", { exact: true })
    .waitFor();
  await page.goto(origin + "/admin/settings");
  await page.getByLabel("Theme", { exact: true }).selectOption("ink");
  await submit(
    page,
    page.getByRole("button", { name: "Save site settings", exact: true }),
  );
  await page.waitForURL(origin + "/admin/settings");
  await visitor.goto(origin);
  assert(
    (await visitor.locator("body").getAttribute("class"))
      .split(" ")
      .includes("ink"),
  );
  const inkColors = await visitor.evaluate(() => ({
    background: getComputedStyle(document.body).backgroundColor,
    text: getComputedStyle(document.body).color,
  }));
  assert.deepEqual(
    inkColors,
    { background: "rgb(20, 35, 40)", text: "rgb(233, 243, 237)" },
    "Ink must apply its background and readable text to the rendered body",
  );
  await visitor.screenshot({
    path: path.join(output, "public-ink.png"),
    fullPage: true,
  });
  // M2: create a parameterized component through the actual visual studio.
  await page.setViewportSize({ width: 1600, height: 1100 });
  await page.goto(origin + "/admin/builder");
  await page
    .getByLabel("Add component identifier", { exact: true })
    .fill("review-card");
  await page
    .getByRole("button", { name: "Add component", exact: true })
    .click();
  await page
    .getByLabel("Add parameter identifier", { exact: true })
    .fill("title");
  await page
    .getByRole("button", { name: "Add parameter", exact: true })
    .click();
  await page.getByLabel("Node type", { exact: true }).selectOption("heading");
  await page.getByLabel("Text", { exact: true }).selectOption("bind");
  await page.getByLabel("Text value", { exact: true }).fill("params.title");
  await page.getByLabel("Template", { exact: true }).selectOption("home");
  await page.getByLabel("Add child", { exact: true }).selectOption("component");
  await page.locator(".outline button").last().click();
  await page
    .getByLabel("Component", { exact: true })
    .selectOption("review-card");
  await page
    .getByLabel("Parameter title value", { exact: true })
    .fill("M2_REUSABLE_CARD");
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "Draft saved." }).waitFor();
  const frame = page.frameLocator('iframe[title="Website draft preview"]');
  await frame
    .getByRole("heading", { name: "M2_REUSABLE_CARD", exact: true })
    .waitFor();
  await visitor.goto(origin);
  assert(!(await visitor.content()).includes("M2_REUSABLE_CARD"));
  await page
    .getByRole("button", { name: "Mobile preview", exact: true })
    .click();
  assert.equal(
    await page
      .locator("iframe")
      .evaluate((e) => Math.round(e.getBoundingClientRect().width)),
    375,
  );
  await page.screenshot({
    path: path.join(output, "m2-studio-mobile-preview.png"),
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Publish theme", exact: true })
    .click();
  await page
    .getByRole("status")
    .filter({ hasText: "Theme published." })
    .waitFor();
  await visitor.reload();
  await visitor
    .getByRole("heading", { name: "M2_REUSABLE_CARD", exact: true })
    .waitFor();
  // Invalid edits stay local and preserve the last valid saved draft.
  await page
    .getByLabel("Node identifier", { exact: true })
    .fill("unsafe identifier");
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await page
    .getByRole("alert")
    .filter({ hasText: "unique safe identifiers" })
    .waitFor();
  await visitor.reload();
  assert((await visitor.content()).includes("M2_REUSABLE_CARD"));
  await page
    .getByLabel("Node identifier", { exact: true })
    .fill("valid-review-instance");
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "Draft saved." }).waitFor();
  // The typed model manager and authoring widget share one field definition.
  await page.getByRole("button", { name: "models", exact: true }).click();
  await page
    .getByLabel("Add model identifier", { exact: true })
    .fill("project");
  await page.getByRole("button", { name: "Add model", exact: true }).click();
  await page
    .getByLabel("Add field identifier", { exact: true })
    .first()
    .fill("client-name");
  await page
    .getByRole("button", { name: "Add field", exact: true })
    .first()
    .click();
  await page.getByRole("button", { name: "Save model", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "Model saved" }).waitFor();
  await page.goto(origin + "/admin/posts/new");
  await page
    .getByLabel("Content type", { exact: true })
    .selectOption("project");
  await page
    .locator("details")
    .filter({
      has: page.getByText("Typed fields & composition", { exact: true }),
    })
    .evaluate((e) => (e.open = true));
  await page
    .getByLabel("client-name", { exact: true })
    .fill("M2_STRUCTURED_CLIENT");
  await page.getByLabel("Title", { exact: true }).fill("M2 project");
  await page.getByLabel("URL slug", { exact: true }).fill("m2-project");
  await submit(
    page,
    page.getByRole("button", { name: "Publish now", exact: true }),
  );
  await page.waitForURL(/\/admin\/posts\/[a-f0-9-]+$/);
  await page
    .locator("details")
    .filter({
      has: page.getByText("Typed fields & composition", { exact: true }),
    })
    .evaluate((e) => (e.open = true));
  await page.getByLabel("client-name", { exact: true }).waitFor();
  assert.equal(
    await page.getByLabel("Content type", { exact: true }).inputValue(),
    "project",
  );
  assert.equal(
    await page.getByLabel("client-name", { exact: true }).inputValue(),
    "M2_STRUCTURED_CLIENT",
  );
  // Progressive widget keyboard behavior uses preloaded panels and local script only.
  const design = await (
    await owner.request.get(origin + "/api/admin/design")
  ).json();
  const active = design.themes.find((t) => t.id === design.active);
  active.package.templates.home.children.push({
    id: "review-tabs",
    kind: "tabs",
    text: "Useful sections",
    children: [
      { id: "review-first", kind: "text", text: "First panel" },
      { id: "review-second", kind: "text", text: "Second panel" },
    ],
  });
  const csrf = await page.locator("input[name=csrf]").first().inputValue();
  const saved = await owner.request.post(
    origin + "/api/admin/design/" + active.id,
    {
      headers: { Origin: origin },
      data: {
        csrf,
        version: active.version,
        package: active.package,
        publish: true,
      },
    },
  );
  assert.equal(saved.status(), 200, await saved.text());
  await visitor.goto(origin);
  const firstTab = visitor.getByRole("tab", {
    name: "First panel",
    exact: true,
  });
  await firstTab.focus();
  await firstTab.press("ArrowRight");
  assert.equal(
    await visitor
      .getByRole("tab", { name: "Second panel", exact: true })
      .getAttribute("aria-selected"),
    "true",
  );
  assert.equal(
    await visitor
      .getByRole("tabpanel")
      .filter({ hasText: "Second panel" })
      .isVisible(),
    true,
  );
  await visitor.screenshot({
    path: path.join(output, "m2-public-components.png"),
    fullPage: true,
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(editorUrl);
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
    "Admin has mobile body overflow",
  );
  await page.screenshot({
    path: path.join(output, "editor-mobile.png"),
    fullPage: true,
  });
  await visitor.setViewportSize({ width: 390, height: 844 });
  await visitor.goto(origin + "/browser-story");
  assert.equal(
    await visitor.evaluate(
      () => document.documentElement.scrollWidth > innerWidth,
    ),
    false,
    "Public page has mobile body overflow",
  );
  await visitor.screenshot({
    path: path.join(output, "public-mobile.png"),
    fullPage: true,
  });
  // M3: native language configuration, translation authoring and publication isolation.
  await page.setViewportSize({ width: 1440, height: 1000 });
  for (const [code, label, direction] of [
    ["fr", "Français", "ltr"],
    ["ar", "العربية", "rtl"],
  ]) {
    await page.goto(origin + "/admin/discovery");
    await page.getByText("Add a language", { exact: true }).click();
    const form = page
      .locator('form[action="/admin/discovery/languages"]')
      .last();
    await form.getByLabel("Language code", { exact: true }).fill(code);
    await form.getByLabel("Language label", { exact: true }).fill(label);
    await form
      .getByLabel("Writing direction", { exact: true })
      .selectOption(direction);
    await form
      .getByLabel("Search button text", { exact: true })
      .fill(code === "fr" ? "Rechercher" : "بحث");
    await submit(
      page,
      form.getByRole("button", { name: "Add language", exact: true }),
    );
    await page.getByText(label + " · " + code, { exact: true }).waitFor();
  }
  await page.goto(origin + "/admin/posts/new");
  await page.getByLabel("Title", { exact: true }).fill("Un jardin tranquille");
  await page.getByLabel("URL slug", { exact: true }).fill("jardin-tranquille");
  await page.getByText("Language & discovery", { exact: true }).click();
  await page.getByLabel("Language", { exact: true }).selectOption("fr");
  await page
    .getByLabel("Search title", { exact: true })
    .fill("Notre jardin · découverte");
  await page
    .getByLabel("Search description", { exact: true })
    .fill("Une publication indépendante, sans compte externe.");
  await submit(
    page,
    page.getByRole("button", { name: "Publish now", exact: true }),
  );
  const translationEditor = page.url();
  await visitor.goto(origin + "/fr/jardin-tranquille");
  assert.equal(await visitor.title(), "Notre jardin · découverte");
  assert.equal(await visitor.locator("html").getAttribute("lang"), "fr");
  assert.equal(await visitor.locator("link[rel=canonical]").count(), 1);
  await page.getByText("Language & discovery", { exact: true }).click();
  await page
    .getByLabel("Search title", { exact: true })
    .fill("PRIVATE SEO WORKING COPY");
  await submit(
    page,
    page.getByRole("button", { name: "Save draft", exact: true }),
  );
  await visitor.reload();
  assert.equal(await visitor.title(), "Notre jardin · découverte");
  await page.goto(origin + "/admin/posts/new");
  await page.getByLabel("Title", { exact: true }).fill("مساحة هادئة للكتابة");
  await page
    .locator(".ProseMirror")
    .fill(
      "أفكار وقصص في مكان مستقل.\n\n## حديقة صغيرة\n\nهذا المحتوى منشور باللغة العربية على خادمك.",
    );
  await page.getByLabel("URL slug", { exact: true }).fill("arabic-story");
  await page.getByText("Language & discovery", { exact: true }).click();
  await page.getByLabel("Language", { exact: true }).selectOption("ar");
  await submit(
    page,
    page.getByRole("button", { name: "Publish now", exact: true }),
  );
  await visitor.setViewportSize({ width: 320, height: 900 });
  await visitor.goto(origin + "/ar/arabic-story");
  assert.equal(await visitor.locator("html").getAttribute("dir"), "rtl");
  assert.equal(
    await visitor.evaluate(
      () => document.documentElement.scrollWidth > innerWidth + 1,
    ),
    false,
  );
  await visitor.screenshot({
    path: path.join(output, "m3-rtl-mobile.png"),
    fullPage: true,
  });
  await page.goto(origin + "/admin/discovery");
  await page.getByLabel("Source path", { exact: true }).fill("/ancien-jardin");
  await page
    .getByLabel("Destination path", { exact: true })
    .fill("/fr/jardin-tranquille");
  await submit(
    page,
    page.getByRole("button", { name: "Add redirect", exact: true }),
  );
  await visitor.goto(origin + "/ancien-jardin");
  assert.equal(visitor.url(), origin + "/fr/jardin-tranquille");
  await page.screenshot({
    path: path.join(output, "m3-discovery-desktop.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 320, height: 900 });
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > innerWidth + 1,
    ),
    false,
  );
  await page.screenshot({
    path: path.join(output, "m3-discovery-mobile.png"),
    fullPage: false,
  });
  // Without JavaScript the same editor must preserve typed SEO controls.
  const native = await browser.newContext({ javaScriptEnabled: false });
  await native.addCookies(await owner.cookies());
  const nativePage = await native.newPage();
  await nativePage.goto(translationEditor);
  await nativePage.getByText("Language & discovery", { exact: true }).click();
  await nativePage
    .getByLabel("Search title", { exact: true })
    .fill("Découverte sans JavaScript");
  await submit(
    nativePage,
    nativePage.getByRole("button", { name: "Publish now", exact: true }),
  );
  await visitor.reload();
  assert.equal(await visitor.title(), "Découverte sans JavaScript");
  await native.close();
  await require("./business_acceptance.cjs")(owner, origin, output);
  await require("./authoring_acceptance.cjs")(owner, origin, output, mediaUrl);
  assert.deepEqual(errors, [], "Browser JavaScript errors");
  assert.deepEqual(remote, [], "Unexpected external runtime requests");
  fs.writeFileSync(
    path.join(output, "result.json"),
    JSON.stringify(
      {
        status: "passed",
        journeys: [
          "public rendering",
          "login/admin",
          "draft/publish",
          "isolated autosave",
          "revision restore",
          "private/public image",
          "comment moderation",
          "theme switch",
          "responsive layout",
          "parameterized visual composition",
          "private responsive draft preview",
          "typed model authoring",
          "invalid draft feedback",
          "keyboard tabs",
          "multilingual discovery and RTL",
          "publication-only SEO",
          "local redirects",
          "native SEO authoring without JavaScript",
        ],
        external_requests: remote.length,
        script_errors: errors.length,
      },
      null,
      2,
    ),
  );
  console.log(
    "PASS: real browser authoring, autosave/revision isolation, media permissions, moderation, theme switching and mobile layouts; no external requests or script errors.",
  );
})()
  .catch(async (e) => {
    console.error(e);
    if (browser) {
      const p = browser.contexts()[0]?.pages()[0];
      if (p) {
        await p.screenshot({
          path: path.join(output, "failure.png"),
          fullPage: true,
        });
        console.error(
          "UI status:",
          await p
            .locator("[data-save-status]")
            .textContent()
            .catch(() => ""),
          "UI error:",
          await p
            .locator("[data-editor-error]")
            .textContent()
            .catch(() => ""),
        );
      }
    }
    process.exitCode = 1;
  })
  .finally(async () => {
    if (browser) await browser.close();
    if (server && server.exitCode === null) {
      server.kill("SIGTERM");
      await new Promise((resolve) => server.once("exit", resolve));
    }
    if (logFd !== undefined) fs.closeSync(logFd);
    fs.rmSync(temporary, { recursive: true, force: true });
  });
