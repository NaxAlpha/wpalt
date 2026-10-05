# Migration and extensions — M8 development

M8 is incomplete. Current core migration tools are an independently tested first slice; plugin/business adapters, owner administration, external APIs/webhooks/worker extensions and complete milestone evidence remain required. See [contract](m8-contract.md).

Owners can open **Operations → Assess a WordPress migration** to upload an export, review a bounded populated assessment and download the complete report. Invalid XML produces a readable retry screen. Uploaded source stays in memory under shared bounded local-processing admission; it is not stored. The native file must be selected again for retry/download.

## Offline assessment

`wpalt wordpress-assess site.xml` reads a UTF-8 WordPress WXR 1.2 export without opening a database or fetching source/media URLs. Review types, counts, URLs, metadata keys and warnings. The report contains source titles/URLs and should be kept private. WXR is not a complete plugin database: a source order record is never proof of a settled payment, and export absence is not proof data did not exist. Retain the original export and independently copied uploads.

XML budgets: 32 MiB input, 100,000 elements, depth 64, 10,000 items and 512 KiB per element. Namespaces resolve by URI, so alternate prefixes work while spoofed namespaces, external entities, DTDs and malformed structures fail. Core identities/fields must be unambiguous. No PHP/shortcode execution.

## Core fresh-target package

Initialize a separate empty migration template with one administrator. This template supplies current configuration/theme/schema and the explicit author mapping; existing site domain records prevent package creation.

```
wpalt --config migration-template.toml init --admin-email owner@example.test
wpalt --config migration-template.toml wordpress-prepare site.xml \
  --owner-email owner@example.test --media-dir independently-copied-uploads
```

Review the plan/counts/warnings, then repeat with `--execute PLAN --output new-private-package.json`. A changed source, mappings, template or media bytes changes the plan. Output is a NEW private file; overwrite is refused. The source/template remain untouched. Packages contain administrator credentials and data: protect them like backups. Restore into a separate empty target with `wpalt --config fresh-target.toml restore new-private-package.json`; occupied targets refuse retries. Validate the recovered site before cutover. Rollback is retaining the source and abandoning the isolated target, not destructive live-site undo.

Supported first slice: posts/pages, structured sanitized HTML projection, category/tag relationships, basic comments, literal Yoast title/description/noindex, exact safe path redirects and explicit owner attribution. Current theme/schema remain the template's. Private/future/pending, password-protected, shortcode-bearing or ambiguous plugin access content stays draft. No imported credentials, entitlement grants, schedules or payment settlement. Unsupported metadata/types are reported; retain/reconcile them through subsequent adapters before cutover. HTML conversion has a conservative 128 KiB/8,192-tag/depth-64 admission boundary; arbitrary theme CSS, widgets, scripts and PHP are not imported by this slice.

Optional `--media-dir` maps `_wp_attached_file` paths within an independently copied uploads directory. Unix descriptor-relative no-follow traversal rejects symlinks/races/path escapes; only regular PNG/JPEG/WebP/GIF originals up to eight MiB/4,096 pixels are embedded. Aggregate media stays within the recovery budget. Orphan/private/ambiguous-parent images remain private. Known image/link source URLs map to local IDs; no server fetch. Missing media directories leave explicit warnings and original external image URLs, which still depend on the source for browser delivery. Derived images, unsupported video and remote media need explicit later mappings. Non-Unix safe-media mapping is not currently supported.

## Verification

`cargo test --locked wordpress_preview_package` runs the connected core journey on SQLite and on real PostgreSQL when `TEST_DATABASE_URL` is configured. It verifies namespace aliases/spoofing, malformed XML/entities/duplicate identities, deterministic previews, unchanged template, private-access/payment safety, local-image bytes, traversal/symlink rejection, fresh recovery, redirects and occupied-target refusal. `python3 scripts/migration_acceptance.py --binary PATH` exercises the actual CLI/private-file/exact-plan boundary. No full migration/extension completion claim.
