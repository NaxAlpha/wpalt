// Reviewable author journey: writing/organization, trustworthy save recovery, then long-document input.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
module.exports = async function (context, origin, output, mediaUrl) {
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const home = process.platform === "darwin" ? "Meta+ArrowUp" : "Control+Home";
  const end = process.platform === "darwin" ? "Meta+ArrowDown" : "Control+End";
  const lineHome = process.platform === "darwin" ? "Meta+ArrowLeft" : "Home";
  const selectLine =
    process.platform === "darwin" ? "Meta+Shift+ArrowRight" : "Shift+End";
  // Delay enhancement to reproduce early typing without discarding it.
  let releaseEnhancement;
  const delayed = new Promise((resolve) => {
    releaseEnhancement = resolve;
  });
  await page.route("**/assets/editor.js", async (route) => {
    await delayed;
    await route.continue();
  });
  await page.goto(origin + "/admin/posts/new", { waitUntil: "commit" });
  await page.getByLabel("Title", { exact: true }).fill("Early writing");
  await page.getByLabel("URL slug", { exact: true }).fill("early-writing");
  await page
    .getByLabel("Content", { exact: true })
    .fill("EARLY_INPUT_PRESERVED");
  releaseEnhancement();
  await page.waitForLoadState("load");
  assert.equal(await page.locator(".ProseMirror").count(), 0);
  await Promise.all([
    page.waitForNavigation(),
    page.getByRole("button", { name: "Save draft", exact: true }).click(),
  ]);
  await page.unroute("**/assets/editor.js");
  assert(
    (await page.locator(".ProseMirror").innerText()).includes(
      "EARLY_INPUT_PRESERVED",
    ),
  );
  const tree = async () =>
    JSON.parse(await page.locator("[name=document]").inputValue()).root;
  const click = (label) =>
    page.getByRole("button", { name: label, exact: true }).click();
  await page.goto(origin + "/admin/posts/new");
  await page
    .getByLabel("Title", { exact: true })
    .fill("A composed writing journey");
  await page.getByLabel("URL slug", { exact: true }).fill("authoring-journey");
  const editor = page.locator(".ProseMirror");
  await editor.fill("Opening line");
  await editor.press("End");
  await editor.press("Enter");
  await editor.press("/");
  await click("Heading 2");
  await page.keyboard.insertText("Café 日本語");
  await page.keyboard.press("Enter");
  await click("Blocks");
  await click("Text");
  await page.keyboard.insertText("Second paragraph");
  assert(
    JSON.stringify(await tree()).includes("Opening line"),
    "opening preserved after block insertion",
  );
  await editor.press(home);
  await editor.press(lineHome);
  await editor.press(selectLine);
  await click("Bold");
  assert.equal(
    await editor.locator("strong").first().innerText(),
    "Opening line",
  );
  await click("Link");
  await page.getByLabel("Link URL", { exact: true }).fill("/about");
  await click("Apply link");
  assert.equal(
    await editor.locator("a").first().getAttribute("href"),
    "/about",
  );
  assert(
    JSON.stringify(await tree()).includes("Opening line"),
    "opening preserved after link",
  );
  await editor.press(end);
  await editor.press("Enter");
  await click("Blocks");
  await click("Callout");
  await page.keyboard.insertText("Remember this");
  assert(
    JSON.stringify(await tree()).includes("Opening line"),
    "opening preserved after callout",
  );
  await click("Blocks");
  await click("New paragraph");
  await click("Blocks");
  await click("Image");
  await page
    .getByRole("button", { name: /A green example image · public/ })
    .click();
  assert.equal(
    await page.getByLabel("Media URL", { exact: true }).inputValue(),
    mediaUrl,
  );
  await page
    .getByLabel("Image description", { exact: true })
    .fill("A green image in the story");
  await click("Insert image");
  assert.equal(
    await editor
      .getByRole("img", { name: "A green image in the story" })
      .getAttribute("src"),
    mediaUrl,
  );
  await editor.press(end);
  await editor.press("Enter");
  await click("Blocks");
  await click("Table");
  await editor.locator("td").first().click();
  await page.keyboard.insertText("Name");
  await page.keyboard.press("Tab");
  await page.keyboard.insertText("Value");
  await click("Add row");
  assert.equal(await editor.locator("tr").count(), 3);
  await click("Add column");
  assert.equal(await editor.locator("tr").first().locator("td").count(), 3);
  await click("Delete column");
  assert.equal(await editor.locator("tr").first().locator("td").count(), 2);
  await click("Organize");
  await editor.locator("p").first().click();
  const before = (await tree()).content.length;
  await click("Duplicate");
  assert.equal((await tree()).content.length, before + 1);
  await click("Undo");
  assert.equal((await tree()).content.length, before);
  await click("Redo");
  assert.equal((await tree()).content.length, before + 1);
  const beforeKeyboard = JSON.stringify(await tree());
  await click("Move down");
  const afterKeyboard = JSON.stringify(await tree());
  assert.notEqual(afterKeyboard, beforeKeyboard);
  assert(afterKeyboard.includes("Café 日本語"));
  // Browser drag/drop exercises the real labeled handle and editor drop target.
  await page
    .getByRole("button", { name: "Drag block", exact: true })
    .dragTo(editor.locator("p").last());
  assert.equal((await tree()).content.length, before + 1);
  assert.notEqual(JSON.stringify(await tree()), afterKeyboard);
  assert(JSON.stringify(await tree()).includes("Opening line"));
  await editor.locator("p").first().click();
  await editor.evaluate((el) => {
    const data = new DataTransfer();
    data.setData(
      "text/html",
      '<p><strong>SAFE_PASTE</strong><script>window.BAD_PASTE=true</script><a href="javascript:alert(1)">Bad link</a><img src="https://remote.invalid/tracker.png"></p>',
    );
    el.dispatchEvent(
      new ClipboardEvent("paste", {
        clipboardData: data,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  assert((await editor.innerText()).includes("SAFE_PASTE"));
  assert.equal(await page.evaluate(() => window.BAD_PASTE), undefined);
  assert.equal(
    await editor.locator('a[href^="javascript:"],img[src^="https:"]').count(),
    0,
  );
  const started = Date.now();
  await Promise.all([page.waitForNavigation(), click("Publish now")]);
  const savedUrl = page.url();
  const publicHtml = await (
    await context.request.get(origin + "/authoring-journey")
  ).text();
  assert(
    publicHtml.includes("<table>") &&
      publicHtml.includes("Remember this") &&
      publicHtml.includes("SAFE_PASTE") &&
      publicHtml.includes(mediaUrl),
  );
  await editor.waitFor();
  assert((await editor.innerText()).includes("Café 日本語"));
  const measures = [];
  for (const width of [320, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(savedUrl);
    await editor.waitFor();
    const box = await editor.boundingBox();
    const values = await editor.evaluate((el) => {
      const s = getComputedStyle(el);
      return {
        font: parseFloat(s.fontSize),
        line: parseFloat(s.lineHeight),
        overflow: document.documentElement.scrollWidth > innerWidth + 1,
      };
    });
    assert(!values.overflow);
    assert(box.height >= 360);
    assert(values.font >= 17 && values.font <= 20);
    assert(
      values.line / values.font >= 1.5 && values.line / values.font <= 1.85,
    );
    measures.push({ viewport_width: width, ...box, ...values });
    await page.screenshot({
      path: path.join(output, `authoring-${width}.png`),
      fullPage: true,
    });
  }
  await page.setViewportSize({ width: 1440, height: 900 });
  // A network interruption keeps an explicit browser recovery copy, not an automatic overwrite.
  await page.route("**/admin/posts/*", (route) =>
    route.request().method() === "POST"
      ? route.abort("failed")
      : route.continue(),
  );
  await editor.fill("UNSAVED_RECOVERY 日本語");
  await page
    .getByText("Offline · changes remain in this editor", { exact: true })
    .waitFor({ timeout: 15000 });
  await page.unroute("**/admin/posts/*");
  page.once("dialog", (d) => d.accept());
  await page.reload();
  await page
    .getByRole("button", { name: "Restore recovery copy", exact: true })
    .waitFor();
  assert(!(await editor.innerText()).includes("UNSAVED_RECOVERY"));
  await click("Restore recovery copy");
  assert((await editor.innerText()).includes("UNSAVED_RECOVERY"));
  await page
    .getByText("Draft saved · live page unchanged", { exact: true })
    .waitFor({ timeout: 15000 });
  assert(
    !(
      await (await context.request.get(origin + "/authoring-journey")).text()
    ).includes("UNSAVED_RECOVERY"),
  );
  // Two tabs prove a stale author cannot overwrite another saved working copy.
  const competing = await context.newPage();
  await competing.goto(savedUrl);
  await competing.locator(".ProseMirror").waitFor();
  await editor.fill("WINNING_WORKING_COPY");
  await page
    .getByText("Draft saved · live page unchanged", { exact: true })
    .waitFor({ timeout: 15000 });
  await competing.locator(".ProseMirror").fill("STALE_WORKING_COPY");
  await competing
    .locator("[data-editor-error]")
    .waitFor({ state: "visible", timeout: 15000 });
  assert(
    (await competing.locator(".ProseMirror").innerText()).includes(
      "STALE_WORKING_COPY",
    ),
  );
  competing.once("dialog", (d) => d.accept());
  await competing.close();
  // Representative long-document input is measured as an observation, not a flaky timing pass.
  const csrf = await page.locator("[name=csrf]").first().inputValue();
  const source = Array.from(
    { length: 1000 },
    (_, i) =>
      `Paragraph ${i}: café 日本語. A useful thought for the independent web.`,
  ).join("\n\n");
  const created = await context.request.post(origin + "/api/admin/content", {
    headers: { "X-CSRF-Token": csrf, Origin: origin },
    data: {
      title: "Long document",
      slug: "long-authoring",
      kind: "post",
      body: source,
      csrf,
      action: "save",
    },
  });
  assert.equal(created.status(), 200);
  const result = await created.json();
  const loadStart = Date.now();
  await page.goto(origin + "/admin/posts/" + result.id);
  await editor.waitFor();
  assert.equal(await editor.locator("p").count(), 1000);
  const loadMs = Date.now() - loadStart;
  await editor.press(end);
  const inputStart = Date.now();
  await page.keyboard.insertText(" END_TYPED");
  const inputMs = Date.now() - inputStart;
  assert(
    (await tree()).content.at(-1).content.at(-1).text.endsWith("END_TYPED"),
  );

  assert.deepEqual(errors, []);
  fs.writeFileSync(
    path.join(output, "authoring-result.json"),
    JSON.stringify(
      {
        status: "passed",
        browser: context.browser().version(),
        journeys: [
          "slash/format/link/multilingual writing",
          "basic table operations",
          "duplicate/undo/redo/keyboard/drag organization",
          "sanitized clipboard payload",
          "canonical publication/reload",
          "failed save and explicit recovery",
          "stale author conflict",
          "1000-paragraph input",
          "320/1440 canvas geometry",
        ],
        long_document: {
          paragraphs: 1000,
          source_bytes: Buffer.byteLength(source),
          load_ms: loadMs,
          input_ms: inputMs,
        },
        canvas: measures,
        elapsed_ms: Date.now() - started,
        limits:
          "Clipboard payload is programmatically supplied. Japanese/Arabic text entry is exercised; native OS IME and screen readers are not certified.",
      },
      null,
      2,
    ) + "\n",
  );
  await page.close();
};
