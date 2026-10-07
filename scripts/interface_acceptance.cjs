// Account-local language switching and stale-tab recovery; content remains independently authored.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, publicContext, origin, output) => {
  const nativeContext = await owner
    .browser()
    .newContext({
      javaScriptEnabled: false,
      storageState: await owner.storageState(),
      viewport: { width: 320, height: 1000 },
    });
  const native = await nativeContext.newPage(),
    stale = await nativeContext.newPage(),
    measured = await owner.newPage();
  const measurements = [],
    accessibility = [],
    errors = [],
    remote = [];
  nativeContext.on("request", (r) => {
    if (!r.url().startsWith(origin)) remote.push(r.url());
  });
  for (const p of [native, stale, measured])
    p.on("pageerror", (e) => errors.push(e.message));
  const preferenceForm = (p) =>
    p.locator("form").filter({ has: p.locator("select[name=locale]") });
  const save = async (p, locale, status = 303) => {
    await preferenceForm(p).locator("select[name=locale]").selectOption(locale);
    const [r] = await Promise.all([
      p.waitForResponse(
        (r) =>
          r.request().method() === "POST" &&
          new URL(r.url()).pathname === "/account/interface",
      ),
      p.waitForNavigation({ waitUntil: "load" }),
      preferenceForm(p).locator("button").click(),
    ]);
    assert.equal(
      r.status(),
      status,
      status >= 400 ? await r.text() : "Save current account locale",
    );
  };
  try {
    const before = await publicContext.request.get(origin + "/");
    const publicBefore = await before.text();
    await native.goto(origin + "/account/interface");
    await stale.goto(origin + "/account/interface");
    await save(native, "fr");
    await save(stale, "ja", 409);
    await stale.getByRole("alert").waitFor();
    assert.equal(
      await preferenceForm(stale).locator("select[name=locale]").inputValue(),
      "fr",
      "Conflict must display current saved state, not overwrite it",
    );
    await measured.route(origin + "/__ui_fixture/axe.js", (r) =>
      r.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(
          path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
        ),
      }),
    );
    for (const locale of ["fr", "ja", "ar", "en"]) {
      if (locale !== "fr") {
        await native.goto(origin + "/account/interface");
        await save(native, locale);
      }
      await measured.goto(origin + "/account/interface");
      assert.equal(await measured.locator("html").getAttribute("lang"), locale);
      assert.equal(
        await measured.locator("#main").getAttribute("lang"),
        locale,
      );
      assert.equal(
        await measured.locator("html").getAttribute("dir"),
        locale === "ar" ? "rtl" : "ltr",
      );
      for (const width of [320, 768, 1440]) {
        await measured.setViewportSize({ width, height: 1000 });
        const m = await ui.geometry(measured);
        assert.deepEqual(
          m.failures,
          [],
          `Interface preference ${locale}/${width}`,
        );
        measurements.push({
          surface: "account language preference",
          locale,
          ...m,
        });
        await measured.screenshot({
          path: path.join(output, `d03-interface-${locale}-${width}.png`),
          fullPage: true,
        });
      }
      await ui.accessibility(measured, origin, "account interface " + locale, {
        accessibility,
      });
      await measured.goto(origin + "/admin/languages");
      assert.equal(await measured.locator("html").getAttribute("lang"), locale);
      assert.equal(
        await measured.locator("#main").getAttribute("lang"),
        "en",
        "Untranslated main workspace must declare its actual language",
      );
    }
    const publicAfter = await (
      await publicContext.request.get(origin + "/")
    ).text();
    assert.equal(
      publicAfter,
      publicBefore,
      "Account language must not alter public projection",
    );
    assert.deepEqual(errors, []);
    assert.deepEqual(remote, []);
    fs.writeFileSync(
      path.join(output, "d03-interface-measurements.json"),
      JSON.stringify(
        {
          browser: owner.browser().version(),
          measurements,
          accessibility,
          errors,
          remote,
          scope:
            "Bundled navigation and native own-account preference; other screen bodies retain English.",
        },
        null,
        2,
      ),
    );
    console.log(
      "PASS: native account interface locale, readable stale-tab conflict, French/Japanese/Arabic/English shell, responsive controls and public projection isolation",
    );
  } finally {
    await nativeContext.close();
    await measured.close();
  }
};
