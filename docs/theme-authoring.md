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
