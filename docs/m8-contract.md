# M8 — Migratable and extensible platform

Active development, 2026-10-05, after user-approved M7 PR #13 merge. No completion claim.

## End-user outcome

Move supported WordPress sites into wpalt with a reviewable assessment, bounded imports, preserved useful relationships/media/URLs, explicit unsupported reports, and checkpoint/recovery paths. Author independent themes and integrations through documented controlled contracts, retaining local ownership. PHP plugins/themes are reference implementations, not executable compatibility targets.

## Working sequence and completion gates

1. Assess WordPress WXR exports offline without network fetches or database writes. Bind execution to exact reviewed source and mappings. Reject malformed/untrusted XML, duplicate identities, oversized/deep inputs and unsafe media paths. Keep unsupported source records recoverable and explain limits.
2. Build validated fresh-target imports and local media mapping, redirects, core author/content/taxonomy/comment relationships. Add explicit selected adapters across existing clusters: ACF fields, SEO metadata, Elementor supported composition mapping, business forms/audience, membership/learning and commerce data. Never synthesize consent, active credentials, financial settlement or access from ambiguous source state. Record exact adapter coverage and source versions.
3. Provide owner-managed revocable least-privilege integration access, bounded versioned APIs and durable webhook/event behavior. Independently authored extensions run outside the application privilege boundary; document configuration, jobs, themes, offline operation and local AI worker integration. Assess freeform theme styles/assets under an explicit trusted-owner boundary. Retain F023/F024 traceability; supported widget mapping must report unsupported output rather than claim universal Elementor pixel/runtime compatibility.
4. Demonstrate an independent integration and theme, portable export/recovery, and migrated reference content plus selected business workflows. Measure counts/relationships/URLs/media/access and operational records; exercise rollback, interrupted/retried import and stale previews.

Every step first works, then receives targeted security/performance/UX verification and optimization; final cumulative SQLite/PostgreSQL/CLI/browser, guidance, compiler-floor, dependency audit and independently verified clean artifact gates remain required. Measure resource bounds/large fixtures and meaningful permission/concurrency failures. No baseline relaxation or pointless combinatorial tests. Deliver one M8 PR and coherent review packet. Distributed execution/upgrade baseline remains M9.

## Initial primary-source review

2026-10-05: WordPress `export_wp` https://developer.wordpress.org/reference/functions/export_wp/ (WXR includes only exportable types, excludes auto-drafts; no database completeness assumption); REST authentication https://developer.wordpress.org/rest-api/using-the-rest-api/authentication/ (capability checks separate from credentials); Elementor data structure https://developers.elementor.com/docs/data-structure/ (JSON element/widget/settings trees); ACF https://www.advancedcustomfields.com/resources/get_fields/ (field values depend on registrations/formatting). Add source-version/test mappings as each adapter is implemented. WXR alone does not contain complete WooCommerce/payment/provider/plugin operational state.
