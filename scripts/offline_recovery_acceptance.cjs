// Actual offline reload and explicit visitor/owner recovery, not mocked storage APIs.
const assert = require("node:assert/strict"),
  fs = require("node:fs"),
  path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, visitors, origin, output) => {
  const admin = await owner.newPage();
  let page = await visitors.newPage();
  const report = {
    status: "running",
    browser: owner.browser().version(),
    geometry: [],
    accessibility: [],
    assertions: [],
    errors: [],
  };
  const errors = (p) => p.on("pageerror", (e) => report.errors.push(e.message));
  errors(admin);
  errors(page);
  for (const p of [admin, page])
    await p.route(origin + "/__ui_fixture/axe.js", (r) =>
      r.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(
          path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
        ),
      }),
    );
  async function measure(p, name, staff = false) {
    for (const width of [320, 768, 1440]) {
      await p.setViewportSize({ width, height: 1000 });
      if (staff) {
        const g = await ui.geometry(p);
        assert.deepEqual(g.failures, []);
        report.geometry.push({ surface: name, ...g });
      } else {
        const g = await p.evaluate(() => ({
          viewport: innerWidth,
          document_width: document.documentElement.scrollWidth,
        }));
        assert(g.document_width <= g.viewport + 1);
        report.geometry.push({ surface: name, ...g });
      }
      await p.screenshot({
        path: path.join(output, `d12-${name}-${width}.png`),
        fullPage: true,
      });
    }
    await ui.accessibility(p, origin, name, report);
  }
  async function post(p, url, body) {
    return p.evaluate(
      async ({ url, body }) => {
        const r = await fetch(url, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        });
        return { status: r.status, value: await r.json() };
      },
      { url, body },
    );
  }
  const definition = {
    title: "A considered enquiry · 日本語",
    fields: [
      {
        name: "name",
        schema: { kind: "string", label: "Your name", required: true },
      },
      { name: "quantity", schema: { kind: "number", label: "Quantity" } },
      {
        name: "private_note",
        schema: { kind: "string", label: "Private note" },
      },
    ],
  };
  try {
    await admin.goto(origin + "/admin/forms");
    await admin.getByLabel("Title", { exact: true }).fill(definition.title);
    await Promise.all([
      admin.waitForNavigation(),
      admin.getByRole("button", { name: "Create form", exact: true }).click(),
    ]);
    await admin.getByLabel("Form title", { exact: true }).waitFor();
    const id = admin.url().split("/").pop(),
      csrf = await admin.locator("#forms-studio").getAttribute("data-csrf");
    assert.equal(
      (
        await post(admin, `/api/admin/forms/${id}`, {
          csrf,
          version: 1,
          definition,
          publish: true,
        })
      ).status,
      200,
    );
    const durations = []; let snapshotBytes = 0;
    for (let sample = 0; sample < 30; sample++) {
      const start = performance.now();
      const response = await visitors.request.get(origin + `/offline/snapshots/form/${id}`);
      const body = await response.body();
      assert.equal(response.status(), 200);
      assert.equal(response.headers()["cache-control"], "no-store");
      assert.equal(response.headers()["referrer-policy"], "no-referrer");
      assert.equal(response.headers()["set-cookie"], undefined);
      assert.equal(JSON.parse(body).form_version, 2);
      snapshotBytes = body.length;
      durations.push(performance.now() - start);
    }
    const ordered = [...durations].sort((a,b) => a-b);
    report.observations = { scope: "30 sequential anonymous HTTP requests, one published three-field scalar form; local fixture, no throughput/SLA claim", snapshot_bytes: snapshotBytes, requests: durations.length, p50_ms: ordered[14], p95_ms: ordered[28] };
    await admin.goto(origin + `/admin/forms/${id}/recovery`);
    await admin
      .getByLabel("Enable recovery for this publication", { exact: false })
      .check();
    await admin
      .getByLabel("Specific recovery purpose", { exact: true })
      .fill(
        "Help complete this enquiry; one requested reminder, no marketing.",
      );
    await admin
      .getByLabel("Selected safe fields · comma separated", { exact: true })
      .fill("name,quantity");
    await Promise.all([
      admin.waitForNavigation(),
      admin
        .getByRole("button", { name: "Publish recovery policy", exact: true })
        .click(),
    ]);
    await measure(admin, "recovery-policy", true);
    await page.goto(origin + `/forms/${id}`);
    await page.getByLabel("Your name", { exact: true }).fill("Initial visitor");
    await page.getByLabel("Quantity", { exact: true }).fill("3");
    await page
      .getByLabel("Private note", { exact: true })
      .fill("Not shared for recovery");
    await page.getByText("Request response recovery", { exact: true }).click();
    await page
      .getByLabel("Recovery reminder email", { exact: true })
      .fill("visitor@example.test");
    await page
      .getByRole("checkbox", { name: /I request owner review/ })
      .check();
    await page
      .getByRole("button", {
        name: "Request partial-response recovery",
        exact: true,
      })
      .click();
    await page.getByText(/Request saved, not submitted/).waitFor();
    await admin.reload();
    assert(
      !(await admin.locator("main").innerText()).includes("Initial visitor"),
    );
    assert(
      !(await admin.locator("main").innerText()).includes(
        "Not shared for recovery",
      ),
    );
    await measure(page, "recovery-pending");
    // Native outbox gives the actual confirmation mail to this isolated fixture's recipient.
    await admin.goto(origin + "/admin/mail");
    const mailLinks = await admin
      .locator('a[href^="/admin/mail/"]')
      .evaluateAll((a) => a.map((n) => n.getAttribute("href")));
    let confirmLink;
    for (const link of mailLinks) {
      await admin.goto(origin + link);
      let body = "";
      for (let attempt = 0; attempt < 60; attempt++) {
        const r = await admin.request.get(origin + link + "/download");
        if (r.ok()) {
          body = await r.text();
          break;
        }
        await admin.waitForTimeout(250);
      }
      body = body
        .replace(/=\r?\n/g, "")
        .replace(/=([0-9a-f]{2})/gi, (_, h) =>
          String.fromCharCode(parseInt(h, 16)),
        );
      const match = body.match(
        /http:\/\/localhost:\d+\/forms\/[a-f0-9-]+#recover=[a-f0-9]{64}&confirm=[a-f0-9]{64}/,
      );
      if (match) {
        confirmLink = match[0];
        break;
      }
    }
    assert(
      confirmLink,
      "Actual queued confirmation must be inspectable by the fixture operator",
    );
    await page.goto(confirmLink);
    await page
      .getByText("Request response recovery", { exact: true })
      .evaluate((summary) => {
        summary.closest("details").open = true;
      });
    await page
      .getByRole("button", {
        name: "Confirm address and recovery permission",
        exact: true,
      })
      .click();
    await page.getByText(/Address and permission confirmed/).waitFor();
    assert(!page.url().includes("recover="));
    await admin.goto(origin + `/admin/forms/${id}/recovery`);
    await admin
      .getByRole("heading", { name: "visitor@example.test", exact: true })
      .waitFor();
    assert(
      (await admin.locator("main").innerText()).includes("Initial visitor"),
    );
    assert(
      !(await admin.locator("main").innerText()).includes(
        "Not shared for recovery",
      ),
    );
    await measure(admin, "confirmed-partial", true);
    await page
      .getByRole("button", {
        name: "Withdraw and delete partial response",
        exact: true,
      })
      .click();
    await page.getByText(/Recovery permission withdrawn/).waitFor();
    await admin.reload();
    assert(
      !(await admin.locator("main").innerText()).includes("Initial visitor"),
    );
    report.assertions.push(
      "Explicit requested/confirmed safe partial response, no unconfirmed owner workspace disclosure, withdrawal/delete; ordinary/private drafts grant no consent.",
    );
    await page.goto(origin + `/offline/#form/${id}`);
    await page.getByText(/Choose Save on this device/).waitFor();
    assert.equal(await page.locator("#library li").count(), 0);
    await page
      .getByRole("button", { name: "Save on this device", exact: true })
      .click();
    await page.locator("#reading #public-form").waitFor();
    await page.getByText("Save and resume", { exact: true }).click();
    await page
      .getByRole("checkbox", {
        name: "Save this draft on this device for seven days",
        exact: true,
      })
      .check();
    await page
      .getByLabel("Your name", { exact: true })
      .fill("Offline preserved visitor · 日本語");
    await page.getByLabel("Quantity", { exact: true }).fill("5");
    await page.waitForFunction(
      (id) =>
        JSON.parse(
          localStorage.getItem(`wpalt:form:${id}`) || "{}",
        ).values?.name?.includes("Offline preserved"),
      id,
    );
    await measure(page, "saved-library");
    const cachesBefore = await page.evaluate(async () => {
      const names = await caches.keys();
      const keys = [];
      for (const name of names)
        if (name.startsWith("wpalt-offline-"))
          for (const key of await (await caches.open(name)).keys())
            keys.push(new URL(key.url).pathname);
      return keys;
    });
    assert(
      cachesBefore.every(
        (p) =>
          p === "/offline/" ||
          [
            "/offline/app.js",
            "/offline/style.css",
            "/offline/forms.js",
            `/offline/snapshots/form/${id}`,
          ].includes(p),
      ),
    );
    await visitors.setOffline(true);
    await page.close();
    page = await visitors.newPage();
    errors(page);
    await page.goto(origin + `/offline/#open/form/${id}`);
    await page.getByLabel("Your name", { exact: true }).waitFor();
    assert.equal(
      await page.getByLabel("Your name", { exact: true }).inputValue(),
      "Offline preserved visitor · 日本語",
    );
    assert(
      await page
        .getByRole("button", { name: "Send response", exact: true })
        .isDisabled(),
    );
    await page
      .getByLabel("Your name", { exact: true })
      .fill("Edited while disconnected");
    await page.reload();
    assert.equal(
      await page.getByLabel("Your name", { exact: true }).inputValue(),
      "Edited while disconnected",
    );
    await page.screenshot({
      path: path.join(output, "d12-offline-reopened-320.png"),
      fullPage: true,
    });
    await visitors.setOffline(false);
    await page.goto(origin + `/forms/${id}`);
    await page.getByLabel("Your name", { exact: true }).waitFor();
    assert.equal(
      await page.getByLabel("Your name", { exact: true }).inputValue(),
      "Edited while disconnected",
    );
    // An accepted-but-interrupted reply must retry the exact original payload after republishing.
    await page.route(
      origin + `/api/forms/${id}/entries`,
      async (route) => {
        await route.fetch();
        await route.abort("failed");
      },
      { times: 1 },
    );
    await page
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Retry original response", exact: true })
      .waitFor();
    await page.waitForFunction(
      (id) =>
        JSON.parse(localStorage.getItem(`wpalt:form:${id}`) || "{}").pending
          ?.version === 2,
      id,
    );
    assert.equal(
      (
        await post(admin, `/api/admin/forms/${id}`, {
          csrf,
          version: 2,
          definition: {
            ...definition,
            title: "Revised enquiry",
            fields: [
              ...definition.fields,
              {
                name: "new_required",
                schema: {
                  kind: "string",
                  label: "New required answer",
                  required: true,
                },
              },
            ],
          },
          publish: true,
        })
      ).status,
      200,
    );
    await page.reload();
    await page.getByText(/The published form changed/).waitFor();
    await page
      .getByRole("button", { name: "Retry original response", exact: true })
      .click();
    await page
      .getByText("Your response has been received. Thank you.", { exact: true })
      .waitFor();
    assert.equal(
      await page.evaluate((id) => localStorage.getItem(`wpalt:form:${id}`), id),
      null,
    );
    report.assertions.push(
      "Actual offline close/reopen/edit/reload, online explicit review, exact accepted-but-interrupted retry despite new required field and changed publication; one accepted response.",
    );
    await page.goto(origin + `/offline/#open/form/${id}`);
    await page.getByText(/The live publication changed/).waitFor();
    await page
      .getByRole("button", { name: /Delete A considered enquiry/ })
      .click();
    await page.waitForFunction(()=>document.querySelectorAll("#library li").length===0);
    assert.equal(await page.locator("#library li").count(), 0);
    await page
      .getByRole("button", {
        name: "Clear library and device form drafts",
        exact: true,
      })
      .click();
    await page
      .getByText("Library and device form drafts cleared.", { exact: true })
      .waitFor();
    assert.equal(
      await page.evaluate(
        async () => navigator.serviceWorker.controller.scriptURL,
      ),
      origin + "/offline/worker.js",
    );
    await page.goto(origin + `/forms/${id}`);
    assert.equal(
      await page.evaluate(() => navigator.serviceWorker.controller),
      null,
      "Worker cannot control ordinary forms/admin/account navigation",
    );
    report.assertions.push(
      "Explicit snapshot admission, cache route allowlist, changed-source warning, individual delete/clear, and worker scope excludes live forms/admin.",
    );
    // A real published article keeps only public text; expiry is a labeled persisted-state fixture.
    await admin.goto(origin + "/admin/posts/new");
    const contentCsrf = await admin.locator("input[name=csrf]").first().inputValue();
    async function saveArticle(articleId, value) {
      const response = await owner.request.post(origin + "/api/admin/content" + (articleId ? "/" + articleId : ""), {
        headers: { Origin: origin, "X-CSRF-Token": contentCsrf },
        data: { csrf: contentCsrf, kind: "post", fields: "{}", ...value },
      });
      assert(response.ok(), `Article operation ${response.status()}`);
      return response.json();
    }
    const article = await saveArticle(null, { title: "A quiet public reading", slug: "d12-quiet-reading", body: "# Public reading\n\nOffline article text · 日本語", action: "publish" });
    const working = await saveArticle(article.id, { title: "PRIVATE WORKING TITLE", slug: "d12-quiet-reading", body: "PRIVATE WORKING BODY", action: "save", version: article.version });
    const source = `article/${article.id}`;
    await page.goto(origin + `/offline/#${source}`);
    await page.getByText(/Choose Save on this device/).waitFor();
    await page.getByRole("button", { name: "Save on this device", exact: true }).click();
    await page.locator("#reading h2").waitFor();
    assert((await page.locator("#reading").innerText()).includes("Offline article text"));
    assert(!(await page.locator("#reading").innerText()).includes("PRIVATE WORKING"));
    await visitors.setOffline(true);
    await page.reload();
    await page.getByText(/Offline copy; current publication cannot be checked/).waitFor();
    assert((await page.locator("#reading").innerText()).includes("日本語"));
    await page.screenshot({ path: path.join(output, "d12-offline-article.png"), fullPage: true });
    await visitors.setOffline(false);
    await saveArticle(article.id, { title: "PRIVATE WORKING TITLE", slug: "d12-quiet-reading", body: "PRIVATE WORKING BODY", action: "unpublish", version: working.version });
    await page.reload();
    await page.getByRole("alert").filter({ hasText: /live publication is unavailable/ }).waitFor();
    assert(await page.locator("#reading").isHidden());
    assert.equal(await page.evaluate(async (source) => (await caches.open("wpalt-offline-v1-public")).match(`/offline/snapshots/${source}`).then(Boolean), source), false);
    await page.screenshot({ path: path.join(output, "d12-live-denial.png"), fullPage: true });
    // Inject an expired, otherwise genuine saved record; no artificial clock or mock cache implementation.
    await page.goto(origin + `/offline/#form/${id}`);
    await page.getByText(/Choose Save on this device/).waitFor();
    await page.getByRole("button", { name: "Save on this device", exact: true }).click();
    await page.locator("#reading #public-form").waitFor();
    await page.evaluate(async (id) => {
      const cache = await caches.open("wpalt-offline-v1-public"), key = `/offline/snapshots/form/${id}`;
      const record = await (await cache.match(key)).json();
      record.expires = Date.now() - 1000;
      record.saved = record.expires - 7 * 86400000;
      await cache.put(key, new Response(JSON.stringify(record), { headers: { "content-type": "application/json" } }));
    }, id);
    await page.reload();
    await page.getByRole("alert").filter({ hasText: /not saved or has expired/ }).waitFor();
    assert.equal(await page.locator("#library li").count(), 0);
    // Invalid source is rejected before any network/cache admission, with keyboard form submission.
    await page.locator("#source").fill("../admin");
    await page.locator("#source").press("Enter");
    await page.getByRole("alert").filter({ hasText: /Use article\/identifier/ }).waitFor();
    assert.equal(await page.locator("#library li").count(), 0);
    report.assertions.push("Actual public article save/disconnected reopen excludes working text; real unpublication denies and deletes its saved copy; persisted-expiry fixture is removed; invalid source/keyboard submission cannot cache private routes.");
    // Exercise the real count ceiling using actual published resources and the visitor interface.
    for (let item = 0; item < 31; item++) {
      const created = await saveArticle(null, { title: `Finite reading ${item + 1}`, slug: `d12-finite-reading-${item}`, body: `Reading ${item + 1}`, action: "publish" });
      await page.locator("#source").fill(`article/${created.id}`);
      await page.getByRole("button", { name: "Save on this device", exact: true }).click();
      if (item < 30) await page.waitForFunction((count) => document.querySelectorAll("#library li").length === count, item + 1);
      else await page.getByRole("alert").filter({ hasText: /Library full/ }).waitFor();
    }
    assert.equal(await page.locator("#library li").count(), 30);
    await page.getByRole("button", { name: "Clear library and device form drafts", exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll("#library li").length === 0);
    report.assertions.push("31 actual publications exercise the 30-item ceiling through visitor controls; excess admission preserves all saved items, and Clear removes the library.");
    // A genuine first installation with altered stylesheet bytes must fail closed and explain the wait.
    const failedContext = await owner.browser().newContext();
    try {
      await failedContext.route(origin + "/offline/style.css", route => route.fulfill({ contentType: "text/css", body: "/* controlled changed release asset */" }));
      const failedPage = await failedContext.newPage(); errors(failedPage);
      await failedPage.goto(origin + "/offline/");
      await failedPage.getByRole("alert").filter({ hasText: /Offline storage could not be prepared/ }).waitFor({ timeout: 25000 });
      assert.equal(await failedPage.evaluate(() => navigator.serviceWorker.controller), null);
      await failedPage.getByRole("link", { name: "Visit live site", exact: true }).click();
      assert.equal(new URL(failedPage.url()).pathname, "/");
      report.assertions.push("Controlled changed shell asset prevents first worker activation; real setup deadline reports failure and ordinary live navigation remains usable. This is a shell-integrity failure fixture, not an OS storage-denial claim.");
    } finally { await failedContext.close(); }
    assert.deepEqual(report.errors, []);
    report.status = "passed";
  } catch (e) {
    report.status = "failed";
    report.failure = e.message;
    throw e;
  } finally {
    await visitors.setOffline(false);
    fs.writeFileSync(
      path.join(output, "d12-offline-recovery.json"),
      JSON.stringify(report, null, 2),
    );
    await admin.close();
    await page.close();
  }
};
