# M3 — Multilingual discovery platform

Status: implementation contract, 2026-10-01. M1/M2 and frontend PRs #4/#5 are merged. This milestone delivers owner-hosted multilingual publishing, language-scoped discovery, SEO output and safe redirect/link administration. It does not claim all researched SEO plugins' entire capability sets or search-engine ranking gains.

## End-user system

Configure named languages, direction and per-language navigation; author independently publishable translations connected by a shared group. Default-language URLs remain at the root; other configured languages use explicit language prefixes. Missing/unpublished translations do not appear in language navigation or hreflang. Core content slugs remain globally unique; translated slugs are authored explicitly. Language/SEO values share content revisions, autosave, publication, scheduling and recovery.

Provide per-content title/description overrides, indexing policy and supported Article/WebPage structured output, with a single metadata owner. Generate canonical, reciprocal published hreflang, social metadata, robots.txt and bounded publication-dependent sitemap pages. Canonical paths derive from actual local publication; no arbitrary external canonical URL or vendor account. Configure visible local-business identity/address information and emit matching structured data without fabricating ratings.

Visitors can navigate translations and search published content by language using the existing indexed search paths. Content collections respect the selected language; explicitly authored relationships remain shared. Administrators manage exact local redirects with conflict/loop/work-limit checks and inspect bounded local published link checks. No server-side arbitrary URL fetching or external crawler requirement. Optional external performance data is deferred as an integration, without blocking local functionality.

## Verification

Readable clusters cover publication/translation isolation and missing variants; one canonical/metadata owner, sitemap/robots exclusion and escaping; redirect conflicts/loops and permission/CSRF boundaries; backup, migration, schedules/revisions and optimistic edit races. Run real SQLite/PostgreSQL paths. Browser journeys exercise language/SEO authoring and discovery controls in the calm cardless design. Measure search/sitemap query plans and realistic volume, bounded work and request timings; report observations and limitations.

## Lifecycle

Schema 3 preserves existing schema-2 data with a one-off migration; no old runtime parser mode. Previous revision envelopes are upgraded once. Backups adopt the current schema; document restoration of old backups through the matching old binary then database migration. Record applicable Google/W3C/sitemap primary guidance, dates, mapped tests and review triggers. A milestone PR is ready only after complete current-head application/compiler/build checks and verified clean artifact; merge remains a separate user decision.
