# Compose a site in M2

Open **Design studio** as an administrator. Theme/schema/options writes require the site's ordinary local admin session and CSRF protection; editors may edit content and inspect authenticated draft previews. No vendor account or network service is involved.

Select a theme and template, then select nodes in its outline. Add/move/remove children and edit their properties. Templates cover home, search, generic content and any installed model. Header/footer are shared roots. Add reusable components, declare parameters and insert component instances with explicit values or bindings. Editing the component changes every instance after deliberate publication.

Choose literal values or bindings such as `site.title`, `post.fields.subtitle`, `options.announcement`, `item.title`, `item.fields.subtitle` or `params.title`. A model-specific template can traverse `post.fields.client.title` when `client` is a declared relationship. Repeater children use `item.label`; flexible sections expose `item.type` and `item.values.field`. Nested field paths must exist in installed definitions. Missing/unpublished related records and non-public images resolve to empty in visitor output. Conditional visibility is presentation, not an authorization policy.

Conditions support truthiness, equality/inequality, numeric greater/less comparisons, and bounded `all`/`any`/`not` trees. The visual inspector exposes single predicates; nested expressions can be edited through the validated package editor. Example:

```json
{"op":"eq","left":{"bind":"item.type"},"right":"hero"}
```

Choose grid/row/stack layout, columns and mobile columns, spacing, width and heading levels. Advanced validated packages also expose hex colors and alignment. Global tokens are background, panel, text, muted, accent and font (`system`, `serif`, `mono`). URLs cannot execute scripts; images reference uploaded UUIDs. Theme packages cannot inject event handlers, scripts, arbitrary CSS, files or network loaders.

Draft changes autosave after a short idle period. Validation errors preserve the last valid saved draft. The preview uses the same server renderer and draft options, in a sandbox without theme script execution. Tabs therefore show all readable sections in preview; their local progressive interaction runs on the published site. The carousel supports native horizontal scrolling, with no auto-rotation. Choose desktop/mobile preview and an existing content record for model templates.

**Publish theme** updates its visitor snapshot; **Activate** chooses an already published package. Theme activation does not publish shared option drafts. Publish options separately. Restore a historical theme revision into a new draft, inspect it and publish deliberately. Stale saves return a conflict and preserve both stored data and the local edit; reload/reapply your edit. A restored package with removed field dependencies must be repaired before it can become a current draft.

Under **models**, define custom content types, taxonomies, model-specific fields, common fields, reusable field groups and shared option definitions. Supported kinds: string, number, boolean, media, relationship, object, group, repeater, gallery and flexible variants. Structured content widgets derive from these same definitions. Add an optional field first, populate working/live records, then make it required. Definitions with existing incompatible values, reference targets, terms or installed theme bindings cannot be silently removed or changed. Large schema changes require an explicit offline migration; limits are safety ceilings, not an unlimited schema system.

JSON import/export is a portable theme graph, not a whole-site export. Destination models/field groups/options must be installed first, and media UUIDs need appropriate destination media or replacement. Import validation reports missing dependencies; full backups move design plus content/data/media. Imported themes begin as private drafts. Both built-in themes are exported through the same format.

Offline commands use the same parser and publication rules. Stop the server first; the site process lock prevents concurrent CLI mutations:

```sh
wpalt --config wpalt.local.toml theme export paper paper.json --draft
wpalt --config wpalt.local.toml theme import custom custom.json
wpalt --config wpalt.local.toml theme publish custom
wpalt --config wpalt.local.toml theme activate custom
```

Exports create a new private file and refuse overwrites. Advanced package editing uses **Validate and save package**; an invalid JSON/package does not replace the valid graph.

Current graph ceilings: 32 models/themes/components, 512 unique definition nodes, depth 12, expanded/rendered work 5,000 nodes, 50 items per collection/repeater, 128 distinct related records/media, four relationship loading rounds, 2 MiB aggregate render data and document output, 1 MiB assembled section, 256 KiB package. A request loads only the selected template's component dependencies. These are intentional development limits; multi-server coordination and arbitrary executable extensions are later milestones.

## Published form composition (M4)

The `form` node takes a literal published form UUID in `text`. The visual builder offers a published-form picker. Saving verifies referenced forms with a bounded batched query; arbitrary URLs or visitor-selected destinations are not accepted. Public output uses a same-origin iframe with only the form, shared styles and automatic value-free sizing. Theme draft previews show a placeholder. Canonical body documents can also contain form blocks; email projects them to absolute links.

## Classified directories and typed validation (D01)

Studio → models defines the shared schema; Content → Typed fields & composition authors the values. Existing stored values are checked before any definition change. An incompatible change leaves the original definitions and content intact; migrate values explicitly rather than deleting them implicitly.

New kinds are `integer`, `email`, `url`, `date`, `choice`, `choices`, and `relationships`. Integers use JavaScript's exact safe-integer range. `min`/`max` constrain number/integer fields; `min_length`/`max_length` constrain text/email/URL/date by Unicode scalar count, with an independent 8,000-byte text ceiling. Server validation remains authoritative. Email syntax does not verify identity. URLs admit HTTP(S) without embedded credentials or whitespace; storing a URL does not fetch it. Dates use real calendar dates in `YYYY-MM-DD`, years 0001–9999.

Choices map stable lowercase identifiers to display labels, with at most 64 choices. `choices` and `relationships` preserve selection order, reject duplicates and enforce `max_items` (1–50, default 20). Multiple relationships name one installed target model. They use the existing 128-distinct-reference record ceiling and bulk loading; private/unpublished targets remain absent from visitor output. Selectors currently expose the latest 128 records/media; larger searchable selectors remain a later usability improvement. A template can use a `repeater` with `source: "post.fields.partners"` and child text bound to `item.title` to display authorized related records.

Example model definition for the existing owner API `POST /api/admin/models/directory` (envelope: `csrf`, reviewed `version`, `definition`; version 0 creates a model):

```json
{
  "label": "Directory",
  "fields": {
    "rating": {"kind": "integer", "min": 1, "max": 5},
    "contact": {"kind": "email"},
    "opened": {"kind": "date"},
    "level": {"kind": "choice", "choices": {"local": "Local", "regional": "Regional"}},
    "partners": {"kind": "relationships", "target": "post", "max_items": 10}
  },
  "taxonomies": {"sector": "Sectors"},
  "taxonomy_parents": {"sector": {"orchards": "gardens", "gardens": "outdoors"}}
}
```

Hierarchy edges use term URL slugs, not names or IDs. Define each shared taxonomy tree on one model; other models declaring the same taxonomy inherit it. Parents can be declared before any content is assigned. Maps have at most 128 edges, eight-edge paths and no cycles. Setting a parent does not assign a draft term to public content. Parent archives include published descendants through `/?taxonomy=sector&term=outdoors` or `/api/content?taxonomy=sector&term=outdoors`; language routes and cursor links retain filters. Existing `?category=`/`?tag=` URLs continue to work and include descendants. Publication, access and noindex filters still apply. These are faceted listing URLs under the current discovery policy, not a new promise of dedicated taxonomy sitemap/SEO pages.

Use the schema-17 executable for these definitions. Stop all processes and execute the reviewed upgrade with an encrypted pre-change recovery point when upgrading native schema 14/15/16. Ordinary runtime and inspection/domain CLI commands refuse an older native schema. The portable table envelope is v13; explicit offline conversion is required for retained v12 archives; older executables reject the expanded definition grammar. Roll back using the pre-change recovery point and retained old executable into a fresh target. No old feature runtime or vendor account is required.


## Independent authoring and bounded local layout proposals (M8)

`examples/themes/field-journal.json` is an independently authored native package, using shared navigation, cardless listings, a readable article template and existing local assets. `theme validate FILE` checks grammar, installed definitions and literal dependencies without saving/publishing the package. It uses the site's stopped-host CLI boundary; initialize/configure the target first. Import remains draft, then inspect it in Studio before explicit publication and activation. `item.excerpt` and `post.excerpt` derive bounded plain text from already authorized body projections without additional queries. Their Markdown parse input is capped at 220 characters; truncated summaries end at a word boundary with an ellipsis. No private relationship lookup is inferred.

Public theme typography and control sizing have their own reviewed contract (`scripts/theme-contracts.json`): body/control type 16px, body line height 1.6, targets at least 44px, radius 6px and visible focus outline 3px, with native package font-family choice retained. The public main target receives keyboard focus from the skip link. Admin tokens/baselines remain independent. The independent-theme browser journey measures populated home/content at 320/768/1440, exact responsive columns/gap and bounded reading widths, text spacing, keyboard navigation and scoped accessibility. Human inspection remains necessary for aesthetics.

The separately operated local AI worker can choose preapproved palette/font/reading-width/listing presentation options on an owner-supplied native base. It cannot add executable code, arbitrary links, assets or access. Its private source-bound proposal still requires exact owner review, native validation, draft import and deliberate publication. This is a bounded layout generator; not arbitrary theme synthesis or a semantic/aesthetic quality guarantee. See `examples/integrations/README.md` and `scripts/local_ai_reference.py`.

### Trusted freeform styles and assets assessment

Raw CSS is feasible locally and does not need a hosted account. It is a stronger trust boundary than declarative layout: CSS can load resources, track visitors, obscure controls or change page meaning. The current package grammar deliberately does not admit raw CSS/JS, event handlers or plugin/PHP execution. This assessment does **not** claim a raw-CSS loader was implemented. The native graph covers bounded color/type/layout/responsive styles, local widgets and authorized media references. Public and sandbox-preview CSP remains server controlled; the theme cannot relax it.

If a future raw-style mode is implemented, admission must be an explicit site-owner configuration plus owner-reviewed package action, served as a separate local style asset, scoped to visitor/sandbox surfaces and bounded by bytes/selectors/loading work. Network URLs/imports, private media, same-origin requests and local font/file loading require their own allowlisted asset capabilities and review, even when scripts are denied by CSP. Unrestricted files and executable JavaScript must not arrive through a style capability. Public upload/media UUIDs already follow visibility/membership protection, derivative/privacy budgets and preload rules; packages cannot turn private files into public assets. Fonts currently use local system families; arbitrary uploaded font formats and third-party remote font services are not implied. Preserve raw source assets and explain unsupported conversions rather than guessing equivalent rendering. No compatibility runtime or vendor activation is required for independent native themes.


## Reviewed publication

An owner enables “Require assigned review before publication” on a model in Design studio. Its writers save and request review from a different active editor/administrator. The assigned reviewer opens the local Editorial queue, inspects the saved content and approves or requests changes privately. Publication and scheduling require approval of that exact canonical document, structured fields, classifications, language and discovery metadata under the current definition. Content and policy edits require fresh review. Editors retain their current broad access; this is not a restricted contributor role.

The native “Save & request review” button includes unsaved edits in one transaction. Decision controls refer to the saved working copy; save or discard on-screen edits before deciding. Private notes and recent decisions never appear in the public content projection. Review history is bounded to twenty decisions per item; the queue uses cursor pagination. Local review works without external accounts or email. Reviewer choices initially show at most 128 active accounts; large-team search and custom multi-stage policies remain later scope.

A scheduled promotion rechecks exact material and current participants. If approval becomes invalid, promotion stops and preserves any earlier live snapshot. Editing scheduled reviewed content cancels its pending promotion. Cloned presentation content loses publication approval and requires a new destination review; source feedback/history remain private. Backups and fresh recovery include review authority under portable v13 and validate actors, versions, shapes and bounded notes before writes.

## D04 grammar — delivery verification in progress

Theme package 2 adds theme-owned navigation families, reusable bounded styles and static local TrueType font faces. Named navigation shares draft/live versioning; the existing empty-source site-links selection still follows site settings. Fonts reference immutable SHA-256 asset identities stored once outside revision JSON. Uploads require an owner, source and distribution-permission/license statement. Only inspected static TrueType containers up to 2 MiB are admitted initially; WOFF/WOFF2, variable, color, SVG and CFF fonts are refused. A theme declares at most eight faces with inspected weight/style, `swap` or `optional` loading and a controlled system/serif/mono fallback. `local:<face>` selects its site typeface.

The staging runtime uses native schema 19, portable recovery v15 and package 2. Ordinary runtime/restore accepts the current formats. Explicit stopped upgrade handles native 14–18, retaining the original encrypted recovery bytes for fresh rollback with the corresponding old executable. Explicit `migrate-recovery-v12`, `migrate-recovery-v13` and `migrate-recovery-v14` produce separately validated current archives. Do not overwrite an original recovery point. Actual retained-native-18 migration and fresh old-runtime rollback pass on both database engines; final exact-head CI/release gates remain in verification; the previous paragraph describes the earlier released D01 boundary.

Published font access follows the current published version of each declaring theme. Drafts and retained history alone grant no anonymous access. Browser caches may retain bytes already delivered publicly; withdrawing a reference controls subsequent server requests and does not erase previously downloaded public files. Backups preserve font bytes, immutable metadata and exact revision references; repeated revisions do not embed copies of the blob.

Reusable styles are bounded named compositions (at most 32) applied with `style_ref`. An attached node does not mix independent overrides; detach a copy before independent editing. Properties include responsive layout/columns/gap/padding/width, controlled colors/alignment, node typeface, 12–96px type, 100–900 weight, 100–240% line height, logical block margin up to 96px, border up to 8px and radius up to 48px. Zero/empty inherits the native default. Raw CSS, imports and remote asset loaders are not admitted. Component roots participate in reachable template stylesheets, so unused component styles do not add request bytes.

Native font-inclusive bundles include exactly the selected theme's declared fonts, their original bytes and immutable provenance/license metadata. They do not import content, media images, forms or external files; their native references must already exist on the destination. Full site recovery remains the complete graph transfer. Bundle import validates containers, descriptors, current dependencies and quotas; inserts assets, the theme and revision references in one transaction. A conflict or failure commits none of the new graph. Imports are private drafts by default. Review bundled distribution rights before confirming import. Export saved drafts from the Studio; unsaved plain JSON export still carries declarations without binary fonts.

Stopped-site CLI examples:

```sh
wpalt --config site.toml theme font-import font.ttf --label 'My local font' --source 'Owner reference' --license LICENSE.txt --rights
wpalt --config site.toml theme bundle-export my-theme theme.wpalt-theme.json --draft
wpalt --config destination.toml theme bundle-import imported-theme theme.wpalt-theme.json --rights
wpalt --config destination.toml theme publish imported-theme
```

Export files are created privately and exclusively. A bundle is limited to 24 MiB; each font to 2 MiB, eight faces per theme, and the shared store to 128 unique fonts / 16 MiB. These are current hard limits. Assets referenced by retained history cannot be deleted; revision retention prunes references alongside the existing 50-plus-live/draft history policy. Explicit manual revision pruning, more font containers, overlay mega-menu templates and unrestricted CSS remain retained breadth.
