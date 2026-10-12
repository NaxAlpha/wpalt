// Actual private upload/acknowledgment, scanner, document and independent verification.
const assert = require("node:assert/strict"),
  fs = require("node:fs"),
  path = require("node:path"),
  crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, visitors, origin, output, { binary, trust }) => {
  const page = await owner.newPage(),
    visitor = await visitors.newPage();
  const report = {
    status: "running",
    browser: owner.browser().version(),
    geometry: [],
    accessibility: [],
    assertions: [],
    document_observations: [],
    errors: [],
  };
  page.on("pageerror", (e) => report.errors.push(e.message));
  visitor.on("pageerror", (e) => report.errors.push(e.message));
  for (const p of [page, visitor])
    await p.route(origin + "/__ui_fixture/axe.js", (route) =>
      route.fulfill({
        contentType: "text/javascript",
        body: fs.readFileSync(
          path.join(__dirname, "../frontend/node_modules/axe-core/axe.min.js"),
        ),
      }),
    );
  async function measure(p, surface, admin) {
    for (const width of [320, 768, 1440]) {
      await p.setViewportSize({ width, height: 1000 });
      await p.waitForLoadState("load");
      if (admin) {
        const m = await ui.geometry(p);
        assert.deepEqual(m.failures, []);
        report.geometry.push({ surface, ...m });
      } else {
        const m = await p.evaluate(() => ({
          viewport: innerWidth,
          document_width: document.documentElement.scrollWidth,
        }));
        assert(m.document_width <= m.viewport + 1);
        report.geometry.push({ surface, ...m });
      }
      await p.screenshot({
        path: path.join(output, `d11-${surface}-${width}.png`),
        fullPage: true,
      });
      await p.screenshot({
        path: path.join(output, `d11-${surface}-${width}-viewport.png`),
      });
    }
    await ui.accessibility(p, origin, surface, report);
  }
  function native(args) {
    const r = spawnSync(binary, args, { encoding: "utf8" });
    assert.equal(r.status, 0, r.stderr);
    return JSON.parse(r.stdout);
  }
  async function verifyDownload(kind) {
    const began = performance.now();
    await page
      .getByLabel("Document format", { exact: true })
      .selectOption(kind);
    await page
      .getByRole("button", { name: "Prepare document", exact: true })
      .click();
    await page
      .getByRole("status")
      .filter({ hasText: "Document checked against" })
      .waitFor({ timeout: 35000 });
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      page
        .getByRole("link", {
          name: "Download verification bundle",
          exact: true,
        })
        .click(),
    ]);
    const bundleFile = path.join(output, `response-${kind}.json`),
      artifactFile = path.join(output, `response.${kind}`);
    await download.saveAs(bundleFile);
    const verified = native([
      "document-verify",
      bundleFile,
      "--trusted-key-sha256",
      trust,
      "--output",
      artifactFile,
    ]);
    assert.equal(verified.valid, true);
    const bundle = JSON.parse(fs.readFileSync(bundleFile));
    assert.equal(
      crypto
        .createHash("sha256")
        .update(Buffer.from(bundle.public_key, "hex"))
        .digest("hex"),
      trust,
    );
    const manifest = JSON.parse(Buffer.from(bundle.manifest, "base64"));
    assert.equal(manifest.source.form_version, 2);
    assert.equal(manifest.source.attachments.length, 1);
    assert.equal(
      manifest.source.fields.find((f) => f.name === "signature").statement,
      "Original terms: accept this exact frozen statement.",
    );
    const pub = Buffer.concat([
      Buffer.from("302a300506032b6570032100", "hex"),
      Buffer.from(bundle.public_key, "hex"),
    ]);
    const publicFile = path.join(output, "server-public.der"),
      messageFile = path.join(output, "attestation-message"),
      signatureFile = path.join(output, "attestation-signature");
    fs.writeFileSync(publicFile, pub);
    fs.writeFileSync(
      messageFile,
      Buffer.concat([
        Buffer.from("wpalt-response-document-attestation-v1\0"),
        Buffer.from(bundle.manifest, "base64"),
      ]),
    );
    fs.writeFileSync(signatureFile, Buffer.from(bundle.signature, "hex"));
    const openssl = process.env.WPALT_TEST_OPENSSL || "openssl",
      independent = spawnSync(
        openssl,
        [
          "pkeyutl",
          "-verify",
          "-pubin",
          "-keyform",
          "DER",
          "-inkey",
          publicFile,
          "-rawin",
          "-in",
          messageFile,
          "-sigfile",
          signatureFile,
        ],
        { encoding: "utf8" },
      );
    assert.equal(independent.status, 0, independent.stderr);
    const [direct] = await Promise.all([
      page.waitForEvent("download"),
      page
        .getByRole("link", {
          name: kind === "pdf" ? "Download PDF" : "Download HTML",
          exact: true,
        })
        .click(),
    ]);
    const directFile = path.join(output, `direct-response.${kind}`);
    await direct.saveAs(directFile);
    assert.deepEqual(
      fs.readFileSync(directFile),
      fs.readFileSync(artifactFile),
    );
    report.document_observations.push({
      kind,
      journey_ms: Math.round((performance.now() - began) * 1000) / 1000,
      artifact_bytes: fs.statSync(artifactFile).size,
      bundle_bytes: fs.statSync(bundleFile).size,
      scope:
        "One serial browser preparation, WebCrypto, direct/bundle downloads, native extraction and OpenSSL; not renderer-only latency or throughput.",
    });
    report.assertions.push(
      `Actual ${kind} bytes and frozen attachment/acknowledgment manifest verified natively and independently by OpenSSL`,
    );
    return artifactFile;
  }
  try {
    await page.goto(origin + "/admin/forms");
    await page
      .getByLabel("Title", { exact: true })
      .fill("D11 private acknowledgment");
    await Promise.all([
      page.waitForNavigation({ waitUntil: "load" }),
      page.getByRole("button", { name: "Create form", exact: true }).click(),
    ]);
    const id = page.url().split("/").pop();
    await page.getByLabel("Form title", { exact: true }).waitFor();
    const csrf = await page.locator("#forms-studio").getAttribute("data-csrf");
    const definition = {
      title: "Private acknowledgment",
      fields: [
        {
          name: "note",
          schema: { kind: "string", label: "Private note", required: true },
        },
        {
          name: "file",
          schema: {
            kind: "string",
            label: "Private attachment",
            required: true,
          },
          widget: { kind: "Upload" },
        },
        {
          name: "signature",
          schema: {
            kind: "object",
            label: "Acceptance",
            required: true,
            fields: {
              name: { kind: "string", required: true },
              accepted: { kind: "boolean", required: true },
            },
          },
          widget: {
            kind: "Signature",
            statement: "Original terms: accept this exact frozen statement.",
          },
        },
      ],
    };
    const save = await owner.request.post(origin + `/api/admin/forms/${id}`, {
      headers: { Origin: origin },
      data: { csrf, version: 1, publish: true, definition },
    });
    assert(save.ok());
    await visitor.goto(origin + `/forms/${id}`);
    await visitor
      .getByLabel("Private note", { exact: true })
      .fill("Zoë · 日本語 · مرحبا <script>forged()</script>");
    await visitor
      .getByLabel("Acceptance: your name", { exact: true })
      .fill("Zoë");
    await visitor
      .getByLabel("I accept this statement and sign using my name", {
        exact: true,
      })
      .check();
    const upload = visitor.getByLabel("Private attachment", { exact: true });
    await upload.setInputFiles({
      name: "controlled.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("WPALT_CONTROLLED_SCANNER_FIXTURE_20261012"),
    });
    await visitor
      .getByRole("alert")
      .filter({ hasText: "scanner detected" })
      .waitFor();
    assert.equal(
      (
        await (
          await owner.request.get(
            origin + `/api/admin/forms/${id}/report?version=2`,
          )
        ).json()
      ).responses,
      0,
    );
    await upload.setInputFiles({
      name: "clean.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("Private clean document."),
    });
    await visitor
      .getByText(
        "Private attachment ready. Send your response within 24 hours.",
        { exact: true },
      )
      .waitFor();
    await measure(visitor, "private-collection", false);
    await visitor
      .getByRole("button", { name: "Send response", exact: true })
      .click();
    await visitor
      .getByRole("status")
      .filter({ hasText: "Your response has been received" })
      .waitFor();
    definition.fields[2].widget.statement = "Changed terms for later visitors";
    assert(
      (
        await owner.request.post(origin + `/api/admin/forms/${id}`, {
          headers: { Origin: origin },
          data: { csrf, version: 2, publish: true, definition },
        })
      ).ok(),
    );
    await page.goto(origin + `/admin/forms/${id}/entries`);
    await page.getByRole("link", { name: /^Response / }).click();
    await page
      .getByRole("heading", { name: "Response document", exact: true })
      .waitFor();
    assert(await page.getByText(trust, { exact: true }).count());
    await measure(page, "response-review", true);
    const html = await verifyDownload("html");
    const content = fs.readFileSync(html, "utf8");
    assert(content.includes("&lt;script&gt;forged()&lt;/script&gt;"));
    assert(!content.includes("<script>"));
    const pdf = await verifyDownload("pdf");
    assert(fs.readFileSync(pdf).subarray(0, 5).equals(Buffer.from("%PDF-")));
    const textFile = path.join(output, "pdf-text.txt");
    const text = spawnSync(
      process.env.WPALT_TEST_PDFTOTEXT || "pdftotext",
      [pdf, textFile],
      { encoding: "utf8" },
    );
    assert.equal(text.status, 0, text.stderr);
    const pdfText = fs.readFileSync(textFile, "utf8");
    assert(
      pdfText.includes("Original terms: accept this exact frozen statement."),
    );
    assert(!pdfText.includes("Changed terms for later visitors"));
    await measure(page, "prepared-document", true);
    await page.getByLabel("Document format", { exact: true }).focus();
    await page.keyboard.press("Tab");
    assert.equal(
      await page.locator(":focus").textContent(),
      "Prepare document",
    );
    const focus = await page.locator(":focus").evaluate((el) => ({
      width: parseFloat(getComputedStyle(el).outlineWidth),
      style: getComputedStyle(el).outlineStyle,
    }));
    assert(focus.width >= 3 && focus.style !== "none");
    const endpoint = new URL(
      await page.locator("[data-attested-document]").getAttribute("action"),
      origin,
    ).href;
    const originalBundle = JSON.parse(
      fs.readFileSync(path.join(output, "response-pdf.json")),
    );
    for (const corruption of ["artifact", "signature", "public_key"]) {
      const altered = structuredClone(originalBundle);
      altered[corruption] =
        corruption === "artifact"
          ? Buffer.from("Changed bytes").toString("base64")
          : "00".repeat(corruption === "signature" ? 64 : 32);
      await page.route(endpoint, (route) =>
        route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(altered),
        }),
      );
      await page
        .getByRole("button", {
          name: "Prepare document",
          exact: true,
        })
        .click();
      await page
        .getByRole("alert")
        .filter({ hasText: "verification failed" })
        .waitFor();
      assert.equal(
        await page
          .getByRole("link", {
            name: "Download verification bundle",
            exact: true,
          })
          .count(),
        0,
      );
      assert.equal(
        await page.getByLabel("Document format", { exact: true }).inputValue(),
        "pdf",
      );
      await page.unroute(endpoint);
    }
    await page.route(endpoint, (route) =>
      route.fulfill({
        status: 422,
        contentType: "application/json",
        body: JSON.stringify({
          error:
            "Controlled renderer unavailable. Your original response remains here.",
        }),
      }),
    );
    await page
      .getByRole("button", { name: "Prepare document", exact: true })
      .click();
    await page
      .getByRole("alert")
      .filter({ hasText: "Controlled renderer unavailable" })
      .waitFor();
    await measure(page, "document-failure", true);
    await page.unroute(endpoint);
    const nativeContext = await owner.browser().newContext({
      storageState: await owner.storageState(),
      javaScriptEnabled: false,
    });
    try {
      const nativePage = await nativeContext.newPage();
      await nativePage.goto(page.url());
      const [download] = await Promise.all([
        nativePage.waitForEvent("download"),
        nativePage
          .getByRole("button", {
            name: "Prepare document",
            exact: true,
          })
          .click(),
      ]);
      const nativeFile = path.join(output, "native-response.json");
      await download.saveAs(nativeFile);
      assert.equal(
        native(["document-verify", nativeFile, "--trusted-key-sha256", trust])
          .valid,
        true,
      );
    } finally {
      await nativeContext.close();
    }
    report.assertions.push(
      "Keyboard focus and prepared/error states retain format and current page; WebCrypto refuses tampered artifacts, signatures and keys before extraction; JavaScript-disabled native bundle verifies independently",
    );
    const staticContext = await owner.browser().newContext();
    try {
      const print = await staticContext.newPage();
      await print.setContent(content, { waitUntil: "load" });
      for (const width of [320, 768, 1440]) {
        await print.setViewportSize({ width, height: 1000 });
        const m = await print.evaluate(() => ({
          viewport: innerWidth,
          document_width: document.documentElement.scrollWidth,
        }));
        assert(m.document_width <= m.viewport + 1);
        report.geometry.push({ surface: "printable-document", ...m });
        await print.screenshot({
          path: path.join(output, `d11-printable-document-${width}.png`),
          fullPage: true,
        });
      }
    } finally {
      await staticContext.close();
    }
    const privateRoute = new URL(page.url()).pathname;
    assert.notEqual(
      (
        await visitors.request.get(origin + privateRoute, { maxRedirects: 0 })
      ).status(),
      200,
    );
    report.assertions.push(
      "Actual isolated scanner refuses controlled detected upload without recording a response, then admits private clean content",
      "Original statement survives republication; script-like private text remains literal in HTML/PDF; respondent identity stays unverified",
    );
    assert.deepEqual(report.errors, []);
    report.status = "passed";
  } catch (error) {
    report.status = "failed";
    report.errors.push(error.message);
    await page
      .screenshot({
        path: path.join(output, "d11-failure-owner.png"),
        fullPage: true,
      })
      .catch(() => {});
    await visitor
      .screenshot({
        path: path.join(output, "d11-failure-visitor.png"),
        fullPage: true,
      })
      .catch(() => {});
    throw error;
  } finally {
    fs.writeFileSync(
      path.join(output, "d11-attested-documents.json"),
      JSON.stringify(report, null, 2),
    );
    await page.close();
    await visitor.close();
  }
};
