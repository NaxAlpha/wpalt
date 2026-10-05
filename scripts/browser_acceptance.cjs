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
let server, browser, logFd, localFixture;
function command(config, args, input) {
  const r = spawnSync(binary, ["--config", config, ...args], {
    input,
    encoding: "utf8",
  });
  assert.equal(r.status, 0, r.stderr);
  return r.stdout;
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
    origin = `http://localhost:${port}`;
  if (process.env.WPALT_LOCAL_PROCESSES) localFixture = await require("./local_process_browser_fixture.cjs")(temporary);
  const config = path.join(temporary, "site.toml");
  const databaseUrl = localFixture ? localFixture.databaseUrl : `sqlite://${temporary}/site.db?mode=rwc`;
  fs.writeFileSync(
    config,
    `database_url = ${JSON.stringify(databaseUrl)}\ndata_dir = "${temporary}/data"\nlisten = "127.0.0.1:${port}"\nbase_url = "${origin}"\n`,
  );
  if (process.env.WPALT_SPAM_ONLY) fs.appendFileSync(config, "\n[spam]\nenabled = true\nproof_bits = 8\n");
  if (process.env.WPALT_SCRIPTS_ONLY) {
    const script=path.join(temporary,'optional-example.js');
    const source='window.__wpaltOptionalExample=(window.__wpaltOptionalExample||0)+1;';
    fs.writeFileSync(script,source);
    fs.appendFileSync(config,`\n[[consent_scripts.scripts]]\nid = "example"\nlabel = "Local example"\npurpose = "Count consented visits on this server."\npath = ${JSON.stringify(script)}\nsha256 = "${crypto.createHash('sha256').update(source).digest('hex')}"\n`);
  }
  if (process.env.WPALT_VIDEO_ONLY) {
    const ffmpeg = process.env.WPALT_TEST_FFMPEG || (process.platform === "darwin" ? "/opt/homebrew/bin/ffmpeg" : "/usr/bin/ffmpeg");
    const ffprobe = process.env.WPALT_TEST_FFPROBE || (process.platform === "darwin" ? "/opt/homebrew/bin/ffprobe" : "/usr/bin/ffprobe");
    assert(fs.existsSync(ffmpeg) && fs.existsSync(ffprobe),"Enabled video browser fixture requires owner-installed tools");
    fs.appendFileSync(config,`\n[video]\nenabled = true\nffmpeg = ${JSON.stringify(ffmpeg)}\nffprobe = ${JSON.stringify(ffprobe)}\n`);
    const fixture = path.join(temporary,"silent-fixture.mp4");
    const generated = spawnSync(ffmpeg,["-v","error","-f","lavfi","-i","color=c=green:s=64x48:r=10:d=1","-an","-c:v","libx264","-threads","1","-pix_fmt","yuv420p",fixture],{encoding:"utf8"});
    assert.equal(generated.status,0,"Generate bounded local silent browser fixture");
    process.env.WPALT_VIDEO_TOOL_VERSION = spawnSync(ffmpeg,["-version"],{encoding:"utf8"}).stdout.split("\n")[0];
    process.env.WPALT_VIDEO_FIXTURE = fixture;
  }
  command(
    config,
    ["init", "--admin-email", "owner@example.test"],
    password + "\n",
  );
  command(config, ["seed-demo"]);
  if (process.env.WPALT_CLONE_ONLY) {
    const archive=path.join(temporary,'source.json'),cloned=path.join(temporary,'held.json');
    command(config,['backup',archive]);
    const preview=JSON.parse(command(config,['recovery-clone',archive,'--source-origin','https://old.example.test']));
    command(config,['recovery-clone',archive,'--source-origin','https://old.example.test','--execute',preview.plan,'--output',cloned]);
    fs.writeFileSync(config,`database_url = "sqlite://${temporary}/clone.db?mode=rwc"\ndata_dir = "${temporary}/clone-data"\nlisten = "127.0.0.1:${port}"\nbase_url = "${origin}"\n`);
    command(config,['restore',cloned]);
  }
  if (localFixture) {
    assert(!process.env.WPALT_CLONE_ONLY, "Clone-only fixture uses its separately supported single-site path");
    await localFixture.start(binary, config, port);
  } else {
    logFd = fs.openSync(path.join(temporary, "server.log"), "w");
    server = spawn(binary, ["--config", config, "serve"], { stdio: ["ignore", logFd, logFd] });
  }
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(origin + "/health")).ok) break;
    } catch (_) {}
    if (server) assert.equal(server.exitCode, null, "Server failed to start");
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
    remote = [],
    passkeyNodes = [];
  if (localFixture) owner.on("response", r => { const route = new URL(r.url()).pathname; if (route.includes("passkeys/") && (route.endsWith("/start") || route.endsWith("/finish"))) passkeyNodes.push({route,node:r.headers()["x-wpalt-node"],status:r.status()}); });
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
  if (process.env.WPALT_THEME_ONLY) { await require("./independent_theme_acceptance.cjs")(owner,publicContext,origin,output); assert.deepEqual(errors,[]); assert.deepEqual(remote,[]); return; }
  if (process.env.WPALT_INTEGRATION_ONLY) { await require("./integration_acceptance.cjs")(owner,origin,output); assert.deepEqual(errors,[]); assert.deepEqual(remote,[]); return; }
  if (process.env.WPALT_MIGRATION_ONLY) { await require("./migration_acceptance.cjs")(owner,origin,output); assert.deepEqual(errors,[]); assert.deepEqual(remote,[]); return; }
  if (process.env.WPALT_SPAM_ONLY) {
    await require("./spam_acceptance.cjs")(owner, origin, output);
    return;
  }
  if (process.env.WPALT_CLONE_ONLY) {
    await require('./clone_acceptance.cjs')(owner,origin,output);
    assert.deepEqual(errors,[]);assert.deepEqual(remote,[]);return;
  }
  if (process.env.WPALT_OPERATIONS_ONLY) {
    await require('./operations_acceptance.cjs')(owner,origin,output,password);
    assert.deepEqual(errors,[]);assert.deepEqual(remote,[]);return;
  }
  if (process.env.WPALT_SCRIPTS_ONLY) {
    await require('./consent_scripts_acceptance.cjs')(owner,publicContext,origin,output);
    assert.deepEqual(errors,[]);assert.deepEqual(remote,[]);return;
  }
  if (process.env.WPALT_VIDEO_ONLY) {
    await require("./video_acceptance.cjs")(owner, publicContext, origin, output, process.env.WPALT_VIDEO_FIXTURE);
    assert.deepEqual(errors,[]);assert.deepEqual(remote,[]);
    return;
  }
  if (process.env.WPALT_OPERATIONS_ONLY) {
    await require("./operations_acceptance.cjs")(owner, origin, output, password);
    return;
  }
  if (process.env.WPALT_COMMERCE_ONLY) {
    await require("./commerce_acceptance.cjs")(owner, origin, output);
    return;
  }
  if (process.env.WPALT_MEMBERSHIP_ONLY) {
    await require("./membership_acceptance.cjs")(owner, origin, output);
    return;
  }
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
  await page.getByLabel("Template", { exact: true }).selectOption("home");
  await page.getByLabel("Add child", { exact: true }).selectOption("image");
  await page.locator(".outline button").last().click();
  await page.getByLabel("Media value", { exact: true }).fill(mediaUrl.split("/").pop());
  await page.getByLabel("Image loading priority", { exact: true }).selectOption("eager");
  const studioLayoutReport = {measurements:[],accessibility:[]};
  const studioUi = require("./ui_contracts.cjs");
  await page.route(origin+"/__ui_fixture/axe.js",route=>route.fulfill({contentType:"text/javascript",body:fs.readFileSync(path.join(root,"frontend/node_modules/axe-core/axe.min.js"))}));
  for (const width of [320,768,1440]) {
    await page.setViewportSize({width,height:1000});
    const measurement = await studioUi.geometry(page);
    assert.deepEqual(measurement.failures,[],`Studio image properties at ${width}`);
    studioLayoutReport.measurements.push(measurement);
    await page.screenshot({path:path.join(output,`studio-image-priority-${width}.png`),fullPage:true});
  }
  await studioUi.accessibility(page,origin,"studio-image-priority",studioLayoutReport);
  fs.writeFileSync(path.join(output,"studio-image-layout.json"),JSON.stringify(studioLayoutReport,null,2));
  await page.setViewportSize({width:1600,height:1100});

  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "Draft saved." }).waitFor();
  const frame = page.frameLocator('iframe[title="Website draft preview"]');
  await frame
    .getByRole("heading", { name: "M2_REUSABLE_CARD", exact: true })
    .waitFor();
  const layoutImage = frame.locator(`img[src="${mediaUrl}"]`);
  await layoutImage.waitFor();
  assert.equal(await layoutImage.getAttribute("loading"), "eager");
  assert(Number(await layoutImage.getAttribute("width")) > 0);
  assert(Number(await layoutImage.getAttribute("height")) > 0);
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
  const publishedLayout = visitor.locator(`img[src="${mediaUrl}"]`);
  await publishedLayout.waitFor();
  assert.equal(await publishedLayout.getAttribute("loading"), "eager");
  assert.equal(await publishedLayout.getAttribute("fetchpriority"), "high");
  assert.equal(await visitor.locator(`link[rel="preload"][as="image"][href="${mediaUrl}"]`).count(), 1);
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
  await require("./engagement_acceptance.cjs")(owner, origin, output);
  await require("./workflows_acceptance.cjs")(owner, origin, output);
  await require("./adversarial_ui.cjs")(owner, origin, output);
  await require("./authoring_acceptance.cjs")(owner, origin, output, mediaUrl);
  await require("./membership_acceptance.cjs")(owner, origin, output);
  await require("./commerce_acceptance.cjs")(owner, origin, output);
  await require("./migration_acceptance.cjs")(owner,origin,output);
  await require("./integration_acceptance.cjs")(owner,origin,output);
  await require("./independent_theme_acceptance.cjs")(owner,publicContext,origin,output);
  await require("./operations_acceptance.cjs")(owner, origin, output, password);
  if (localFixture) {
    const registration = passkeyNodes.filter(r=>r.route.startsWith("/account/passkeys/"));
    const authentication = passkeyNodes.filter(r=>r.route.startsWith("/passkeys/login/"));
    const crosses = [registration,authentication].some(rows=>rows.some(r=>r.route.endsWith("/start") && r.status===200 && rows.some(f=>f.route.endsWith("/finish") && f.status===200 && f.node && f.node!==r.node)));
    assert(crosses, "Actual passkey challenge must finish on a different local process");
  }
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
        ...(localFixture ? { local_processes: {...localFixture.report(), passkey_nodes:passkeyNodes} } : {}),
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
      await new Promise((resolve) => {
        const deadline = setTimeout(() => server.kill("SIGKILL"), 10000);
        server.once("exit", () => {
          clearTimeout(deadline);
          resolve();
        });
      });
    }
    if (localFixture) await localFixture.close();
    if (logFd !== undefined) fs.closeSync(logFd);
    fs.rmSync(temporary, { recursive: true, force: true });
  });
