# 0003 — Owner-hosted multilingual discovery

2026-10-01, implementation decision within the agreed M3 outcome.

Languages and business identity are one versioned site definition. Content language, translation group and SEO join the existing working/publication snapshots and revision envelope. Use one metadata renderer and explicit published translation joins; no independent SEO plugin state or vendor account. Slugs remain globally unique before adoption; each variant has its own authored slug. Shared explicit relationships may cross languages, while discovery collections/search are language-scoped.

Use existing SQLite FTS5/PostgreSQL GIN search and language/date/id indexes. Public requests reuse loaded settings rather than adding duplicate metadata reads. Crawl work is bounded: indexed keyset sitemap pages, a two-scan bounded sitemap index, batched local Markdown destination resolution, and an in-memory bounded exact-redirect graph. No arbitrary network fetch. Site-wide mutation serialization plus optimistic versions/constraints protect current single-server writes; distributed scheduling/mutation coordination remains M9.

Schema 3 migrates meaningful schema-2 content and historical revision envelopes once. Backups use v3 and validate discovery semantics before writes. Restore v2 archives with the corresponding binary, then migrate the database; no permanent old-format runtime parser. Stop writers and retain a pre-upgrade backup; rollback requires the saved old data/binary. Current API callers update metadata together.

Alternatives considered: mandatory provider APIs would break local ownership; separate SEO/translation stores would fragment publication state; a full network crawler introduces SSRF/operational scope beyond local link checking. These choices retain later plugin capability requirements in the parity matrix rather than asserting blanket parity.
