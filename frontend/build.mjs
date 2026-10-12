import { readFile, writeFile } from "node:fs/promises";
import { build, transform } from "esbuild";
const license = await readFile(
  new URL("./node_modules/preact/LICENSE", import.meta.url),
  "utf8",
);
await build({
  banner: { js: "/*! Preact 11.0.0\n" + license + "*/" },
  entryPoints: ["builder.jsx"],
  bundle: true,
  minify: true,
  format: "iife",
  jsxFactory: "h",
  jsxFragment: "Fragment",
  target: ["es2020"],
  outfile: "../assets/generated/builder.js",
  legalComments: "eof",
});

const tokens = JSON.parse(
  await readFile(new URL("./design-tokens.json", import.meta.url), "utf8"),
);
const units = new Set(["space", "radius", "size", "type", "breakpoint"]);
const declarations = Object.entries(tokens)
  .filter(([key]) => key !== "version")
  .flatMap(([group, values]) =>
    Object.entries(values).map(
      ([name, value]) =>
        `--ui-${group}-${name}:${value}${units.has(group) ? "px" : ""}`,
    ),
  )
  .join(";");
let styles = await readFile(new URL("./admin-ui.css", import.meta.url), "utf8");
styles = styles
  .replaceAll("(--ui-mobile)", `(max-width: ${tokens.breakpoint.mobile}px)`)
  .replaceAll(
    "(--ui-studio-stack)",
    `(max-width: ${tokens.breakpoint["studio-stack"]}px)`,
  );
styles = styles.replaceAll(
  "(--ui-studio-single)",
  `(max-width: ${tokens.breakpoint["studio-single"]}px)`,
);
const css = await transform(`:root{${declarations}}\n${styles}`, {
  loader: "css",
  minify: true,
});
await writeFile(
  new URL("../assets/generated/admin-ui.css", import.meta.url),
  "/* Generated frontend design system. */\n" + css.code,
);
await build({
  entryPoints: ["ui-gallery.jsx"],
  bundle: true,
  minify: true,
  format: "iife",
  jsxFactory: "h",
  jsxFragment: "Fragment",
  target: ["es2020"],
  outfile: "../work/ui-gallery.js",
});

const editorLicenses = await Promise.all(
  [
    "prosemirror-model",
    "prosemirror-transform",
    "prosemirror-state",
    "prosemirror-view",
    "prosemirror-history",
    "prosemirror-commands",
    "prosemirror-schema-list",
    "prosemirror-tables",
    "prosemirror-keymap",
    "prosemirror-inputrules",
    "orderedmap",
    "rope-sequence",
    "w3c-keyname",
  ].map(
    async (name) =>
      name +
      "\n" +
      (await readFile(
        new URL(`./node_modules/${name}/LICENSE`, import.meta.url),
        "utf8",
      )),
  ),
);
await build({
  entryPoints: ["editor.mjs"],
  bundle: true,
  minify: true,
  format: "iife",
  target: ["es2020"],
  outfile: "../assets/generated/editor.js",
  banner: { js: "/*!\n" + editorLicenses.join("\n") + "*/" },
  legalComments: "eof",
});

await build({
  banner: { js: "/*! Preact 11.0.0\n" + license + "*/" },
  entryPoints: ["forms.jsx"],
  bundle: true,
  minify: true,
  format: "iife",
  jsxFactory: "h",
  jsxFragment: "Fragment",
  target: ["es2020"],
  outfile: "../assets/generated/forms.js",
  legalComments: "eof",
});

await build({
  entryPoints: ["auth.mjs"],
  bundle: true,
  minify: true,
  format: "iife",
  target: ["es2020"],
  outfile: "../assets/generated/auth.js",
});

await build({
  entryPoints: ["spam.mjs"],
  bundle: true,
  minify: true,
  format: "iife",
  target: ["es2020"],
  outfile: "../assets/generated/spam.js",
});

// One content-bound release shell; worker changes whenever its embedded assets do.
const offlineCss = await transform(
  `:root{${declarations}}\n${await readFile(new URL("../assets/offline.css", import.meta.url), "utf8")}`,
  { loader: "css", minify: true },
);
await writeFile(
  new URL("../assets/generated/offline.css", import.meta.url),
  offlineCss.code,
);
await build({
  entryPoints: ["offline.mjs"],
  bundle: true,
  minify: true,
  format: "iife",
  target: ["es2022"],
  outfile: "../assets/generated/offline.js",
});
const { createHash } = await import("node:crypto");
const offlineAssets = Object.fromEntries(
  await Promise.all(
    [
      ["/offline/", "../assets/offline.html"],
      ["/offline/app.js", "../assets/generated/offline.js"],
      ["/offline/style.css", "../assets/generated/offline.css"],
      ["/offline/forms.js", "../assets/generated/forms.js"],
    ].map(async ([route, file]) => [
      route,
      createHash("sha256")
        .update(await readFile(new URL(file, import.meta.url)))
        .digest("hex"),
    ]),
  ),
);
const offlineBuild = createHash("sha256")
  .update(JSON.stringify(offlineAssets))
  .digest("hex")
  .slice(0, 16);
await build({
  entryPoints: ["offline-worker.mjs"],
  bundle: true,
  minify: true,
  format: "iife",
  target: ["es2022"],
  outfile: "../assets/generated/offline-worker.js",
  define: {
    __WPALT_OFFLINE_BUILD__: JSON.stringify(offlineBuild),
    __WPALT_OFFLINE_ASSETS__: JSON.stringify(offlineAssets),
  },
});
