// Readable local/CI artifact: measurements overlay the actual screenshot at CSS-pixel scale.
const fs = require("node:fs");
const path = require("node:path");
const escape = (value) =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
module.exports = function report(result, directory) {
  const cards = result.geometry
    .map((m, i) => {
      const image =
        m.surface === "shared components"
          ? `components-${m.viewport}.png`
          : m.surface.split("/").filter(Boolean).join("-") +
            `-${m.viewport}.png`;
      const boxes = m.components
        .map(
          (c) =>
            `<span class="box" style="left:${c.x}px;top:${c.y}px;width:${c.width}px;height:${c.height}px" title="${escape(c.name)}: ${c.width.toFixed(1)} × ${c.height.toFixed(1)} CSS px"></span>`,
        )
        .join("");
      const rows = m.components
        .map(
          (c) =>
            `<tr><td>${escape(c.name)}</td><td>${c.width.toFixed(1)} × ${c.height.toFixed(1)}</td><td>${c.x.toFixed(1)}, ${c.y.toFixed(1)}</td><td>${c.radius}px</td><td>${escape(c.font)} / ${escape(c.line_height)}</td></tr>`,
        )
        .join("");
      return `<section id="surface-${i}"><h2>${escape(m.surface)} · ${m.viewport}px</h2><p>${m.failures.length ? escape(m.failures.join("; ")) : "Measured geometry passed."}</p><div class="canvas"><div style="position:relative;width:${m.viewport}px"><img loading="lazy" src="${image}" alt="${escape(m.surface)} at ${m.viewport} CSS pixels" width="${m.viewport}"><div class="overlay">${boxes}</div></div></div><details><summary>Component dimensions and positions</summary><div class="table"><table><thead><tr><th>Control</th><th>Width × height</th><th>x, y</th><th>Radius</th><th>Font / line height</th></tr></thead><tbody>${rows}</tbody></table></div></details></section>`;
    })
    .join("");
  const colors = result.contrast
    .flatMap((v) =>
      v.pairs.map(
        (p) =>
          `<tr><td>${v.viewport}px</td><td>${escape(p.name)}</td><td>${p.ratio.toFixed(2)}:1</td><td>${p.minimum}:1</td></tr>`,
      ),
    )
    .join("");
  fs.writeFileSync(
    path.join(directory, "index.html"),
    `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>wpalt frontend review</title><style>body{font:16px/1.5 system-ui;background:#f5f7f6;color:#172b29;margin:0;padding:24px}main{max-width:1200px;margin:auto}h1{font-size:32px}section{background:white;border:1px solid #d6e0db;border-radius:12px;padding:20px;margin:24px 0}.canvas,.table{overflow:auto;max-height:800px;border:1px solid #71857b}img{display:block}.overlay{position:absolute;inset:0;pointer-events:none}.box{position:absolute;border:1px dashed #185cbb;pointer-events:auto}.box:hover{background:#185cbb20}body:has(#show:not(:checked)) .overlay{display:none}table{border-collapse:collapse;width:100%}th,td{padding:8px;text-align:left;border-bottom:1px solid #d6e0db}summary,label{cursor:pointer;padding:12px;display:block}</style><main><h1>Frontend measurement and visual review</h1><p>Status: ${escape(result.status)} · ${escape(result.platform)} · browser ${escape(result.browser)}</p><p>Sizes and positions are CSS pixels. Screenshots are review evidence; these checks do not assign an aesthetic score. Inspect hierarchy, density, wrapping, focus and task clarity alongside the measurements.</p><label><input type="checkbox" id="show" checked> Show measured control outlines</label><p><a href="measurements.json">Full machine-readable measurements</a></p>${result.error ? `<p>${escape(result.error)}</p>` : ""}<section><h2>Focused contrast samples</h2><table><tr><th>Viewport</th><th>Sample</th><th>Measured</th><th>Minimum</th></tr>${colors}</table></section>${cards}</main></html>`,
  );
};
