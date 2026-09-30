import { readFile } from "node:fs/promises";
import { build } from "esbuild";
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
