# Reviewed Elementor design imports

Design studio → Review an Elementor design import accepts a documented 0.4 JSON export, up to 2 MiB. Save the current native draft first. Choose a new reusable component name and an explicit placement: component only, home, shared header/footer or one existing content item. Shared placements replace that draft region. A specific-content placement preserves the old model template for other items using native conditions. Source siblings are limited to 50, source nesting to 10, and the whole native package to 512 nodes / 256 KiB. Repeated imports consume the same native depth/node budget; large sites may need future per-content template indexing rather than many nested conditions.

Map source image IDs/URLs to existing local assets, source font families to built-in fallbacks or admitted faces and source global references to native reusable styles. Imports never fetch remote media/fonts or grant public access. Existing asset/content access rules govern output. Literal punctuation/code/link destinations are escaped for the native Markdown projection; encoded HTML or Markdown cannot become new executable markup/images. Inline HTML images are omitted with an explicit loss; use mapped native image widgets to retain asset visibility checks. Missing mappings, raw styles, dynamic tags, conditions, animations and unrecognized settings appear in the report. Inspect the source beside this report, acknowledge losses, import privately, edit the reusable component, preview affected pages and publish deliberately. Source children on mapped leaves are lifted into a following native section and explicitly reported, preserving visible order without nested links. Source popup/template assignment remains a reported loss until explicitly chosen in the native draft. Unknown source widgets preserve their independently mapped children in an inert native section; they cannot execute HTML/PHP/scripts.

The defined mappings include ordered legacy containers/sections/columns, heading/text/link/image, native gallery grids and selected tabs/disclosures/dividers/spacers. Atomic div/grid/flex layout and heading/paragraph/button/image use selected typed properties. Layout defaults and supported interactive behavior differ and are reported. Static px/color/typography values use existing controlled native styles; tablet/custom breakpoints and raw atomic variants remain losses. Global colors/typography require explicit owner aliases; missing globals never silently substitute their source fallback. Exact full free/Pro widget/style/template/rendering compatibility remains F024 work; no Pro fixture was executed.

The request and returned report/package determine a theme-bound review fingerprint. Changing source, mappings, placement, component name or base version requires another review. Apply recomputes it and uses the existing current-owner CAS transaction. No endpoint publishes as part of import. The source SHA is canonical JSON data, distinct from the original export file checksum. No new native schema/archive/package format is introduced: schema 19, portable v15, package 2.

## CLI

The owner-operated stopped CLI uses a request JSON containing `source`, `component`, `target`, optional `content_id`/`content_kind`, and `media`, `fonts`, `global_styles` mapping objects. Use existing UUIDs/font aliases/style names. Font map values use native descriptors: `system`, `serif`, `mono` or `local:face-name` (up to 32 source aliases / eight local faces). Source reference keys remain exact strings. The review output is a newly created private file.

```sh
wpalt --config site.toml theme elementor-review paper request.json review.json
wpalt --config site.toml theme elementor-apply paper review.json --acknowledge-losses
```

Review the plan before apply. A modified or stale plan is refused. Save a normal backup before replacing meaningful designs, keep the source, and use the existing Studio/history/recovery workflows. Publication remains separate (`theme publish paper`).

## Verification basis

Pinned free source 4.3.4 (`840f5f54b774b8a04587501d9cd23706dc700024`) and [current atomic structure](https://developers.elementor.com/docs/data-structure/atomic-elements/), [global references](https://developers.elementor.com/docs/data-structure/global-styles/) and [responsive data](https://developers.elementor.com/docs/data-structure/responsive-data/). Native connected SQLite/PostgreSQL and scoped macOS browser behavior are distinct from the dedicated disposable Linux WordPress 7.1.3/free Elementor 4.3.4 export/public-render gate. Pending gates must not be described as executed parity.
