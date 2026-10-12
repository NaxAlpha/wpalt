// Execute the real pinned free calculator and the native canonical preview.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

module.exports = async (owner, origin, output, id, csrf) => {
  const source = path.resolve(
    process.env.WPALT_FORMINATOR_REFERENCE_SOURCE ||
      "work/d10-reference-source",
  );
  const pins = JSON.parse(fs.readFileSync(path.join(source, "pins.json")));
  const bundle = fs.readFileSync(path.join(source, "free-calculator.js"));
  assert.equal(
    crypto.createHash("sha256").update(bundle).digest("hex"),
    pins.bundle_sha256,
  );
  const literal = (value) => ({ operation: "Literal", value });
  const terms = (operation, ...values) => ({
    operation,
    operands: values.map(literal),
  });
  const unary = (operation, value) => ({ operation, operand: literal(value) });
  const binary = (operation, left, right) => ({ operation, left, right });
  const cases = [
    [
      "2+3*4",
      { operation: "Add", operands: [literal(2), terms("Multiply", 3, 4)] },
      14,
      14,
    ],
    [
      "(2+3)*4",
      { operation: "Multiply", operands: [terms("Add", 2, 3), literal(4)] },
      20,
      20,
    ],
    ["min(5,2,8)", terms("Min", 5, 2, 8), 2, 2],
    ["max(5,2,8)", terms("Max", 5, 2, 8), 8, 8],
    ["avg(2,4,6)", terms("Average", 2, 4, 6), 4, 4],
    ["abs(-3)", unary("Abs", -3), 3, 3],
    ["round(-2.5)", { ...unary("Round", -2.5), decimals: 0 }, -3, -2],
    ["ceil(-2.3)", unary("Ceil", -2.3), -2, -2],
    ["floor(-2.3)", unary("Floor", -2.3), -3, -3],
    ["3/0", binary("Divide", literal(3), literal(0)), null, "Infinity"],
  ];
  const context = await owner.browser().newContext();
  const requested = [];
  try {
    const page = await context.newPage();
    await page.route("**/*", (route) => {
      requested.push(route.request().url());
      return route.abort();
    });
    await page.addScriptTag({ path: path.join(source, "free-calculator.js") });
    const results = [];
    for (const [formula, expression, expectedNative, expectedFree] of cases) {
      const free = await page.evaluate((formula) => {
        const result = new window.FreeForminator.default(formula).calculate();
        return Number.isFinite(result) ? result : String(result);
      }, formula);
      assert.equal(free, expectedFree, formula + " actual free behavior");
      const definition = {
        title: "Reference calculator",
        fields: [
          {
            name: "result",
            schema: { kind: "number", required: true },
            calculation: { operation: "Expression", expression },
          },
        ],
      };
      const response = await owner.request.post(
        origin + `/api/admin/forms/${id}/preview`,
        {
          headers: { Origin: origin },
          data: { csrf, definition, values: { result: 999 } },
        },
      );
      const native = await response.json();
      if (expectedNative === null) {
        assert.equal(response.status(), 422);
        assert.match(native.error, /divide by zero/);
      } else {
        assert(response.ok());
        assert.equal(native.values.result, expectedNative);
      }
      results.push({
        formula,
        free,
        native:
          expectedNative === null
            ? { refused: native.error }
            : native.values.result,
        difference:
          formula === "round(-2.5)"
            ? "Native explicitly rounds ties away from zero"
            : formula === "3/0"
              ? "Native refuses non-finite division"
              : "Controlled result agrees",
      });
    }
    assert.deepEqual(
      requested,
      [],
      "Free reference cannot use network services",
    );
    const receipt = {
      source: pins,
      browser: owner.browser().version(),
      cases: results,
      requests: requested,
      scope:
        "Actual source-built free frontend calculator in an isolated browser and actual native canonical HTTP preview. No WordPress PHP/admin/conditional-widget or paid-plugin execution and no universal parity. No plugin code incorporated into wpalt.",
    };
    fs.mkdirSync(output, { recursive: true });
    fs.writeFileSync(
      path.join(output, "d10-forminator-reference.json"),
      JSON.stringify(receipt, null, 2) + "\n",
    );
  } finally {
    await context.close();
  }
};
