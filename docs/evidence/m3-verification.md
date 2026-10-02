# M3 verification record

2026-10-01. Implementation and verification evidence for `milestone/m3-multilingual-discovery`; [delivery PR #6](https://github.com/NaxAlpha/wpalt/pull/6) records final-head CI and clean-artifact status. M2 and calm admin PR #5 are merged; #5's actual-main verification run 36860476634 passed all three jobs.

## Reviewable behavior

The five new high-level journeys in `tests/acceptance.rs` complement existing M1/M2 coverage: multilingual publication/reciprocity/draft isolation and edit races; truthful escaped metadata plus redirect/link/access boundaries; complete recovery with a one-connection pool and malformed/unsafe archive rejection; 2,006-publication sitemap/search volume; schema-2 upgrade preserving live content and editable history. The schema-1 preservation fixture remains exercised. Tests use real HTTP routers and database engines, not SQL-string expectations. Timing observations do not gate correctness by noisy thresholds.

Locally, all 17 acceptance journeys pass on SQLite. Real PostgreSQL 17 is required in PR CI; its result must be checked separately. Browser authoring passes on local Chrome 154 with Playwright 1.62.1: language creation, native SEO, draft isolation, redirects, mobile RTL, and publishing with JavaScript disabled, alongside prior authoring/Studio/media/moderation workflows. No external runtime requests or script errors. UI measurements extend from nine to eleven admin surfaces at 320/768/1440px, plus existing gallery and Studio breakpoints; deterministic gallery baselines remain unchanged. These sampled checks do not certify universal accessibility or all languages/devices.

Primary Google, sitemap and W3C guidance was reviewed 2026-10-01 and mapped in `feature-guidance.json`. The freshness checker gates affected releases; no runtime guideline downloads. Supported output is minimal and truthful, not a Google rich-result/ranking promise. See `../discovery.md` for omitted schema enrichments and broader plugin features.

## Performance and security

`discovery_volume_has_bounded_pages_and_indexed_language_search` generates `work/m3-volume.json`, uploaded as `m3-volume-evidence` in CI. It records each actual engine's sitemap/search query plans and 12 in-process debug request samples for sitemap, index and French search, on 2,006 published rows. Public requests reuse discovery/settings reads; no response cache masks publication changes. Sitemap pages scan at most 1,001 rows, the index performs two bounded scans up to 500,001 keys, and translated alternates are capped at 16. Link resolution batches content/media IDs, examines at most 20 sources/200 destinations, and never fetches external URLs. Redirect graph budget is 1,000/15 hops. Results are workload observations, not production SLOs or a WordPress performance comparison.

Writes require current role/session, CSRF and same-origin checks. Local path restrictions, translation constraints and optimistic versions prevent unsafe or conflicting state; errors do not reveal database values. Semantic archive validation rejects external redirect destinations before writes. SQL/request diagnostics reuse existing correlation/timing/redaction policy. SQLite/PostgreSQL single-server coordination is current; distributed write/scheduler correctness remains M9.

## Visual inspection

The Discovery screen uses existing calm typography, open sections, shared controls and progressive disclosures; non-default language forms collapse to reduce initial density; public RTL retains native writing direction. Review [desktop Discovery](m3/m3-discovery-desktop.png), [mobile Discovery](m3/m3-discovery-mobile.png), [Arabic publication](m3/m3-rtl-mobile.png) and [local geometry summary](m3/local-ui.json); CI retains full browser/UI artifacts. Public utility/comment labels still use English; this is a recorded localization limitation. Native controls have explicit accessible names rather than labels accidentally absorbing option/help text. The redirect action aligns to the field baseline rather than stretching to its grid row's height. Full aesthetic assessment remains human review; automated dimensions and pixels are supporting evidence.

## Migration and limits

Schema 3 upgrades metadata and old revisions once. Current backups are v3; restore older archives with the matching old binary and then migrate its database. No compatibility shim added. Default language changes are allowed only before content; globally unique slugs and registered-code author responsibility are explicit. No automatic translation, provider search-performance connector, custom schema graph, regex redirects, external crawler, global orphan graph or full premium SEO plugin parity is claimed.

## Native release measurements

[Recorded SQLite release workload](m3/release-performance.json): macOS ARM64, 3,000 seeded stories plus About, 30 requests per endpoint/concurrency, local Python HTTP client, warm application, uncompressed full documents, no response cache. Median/p95 observations in milliseconds:

| Endpoint | Concurrency 1 median / p95 | Concurrency 10 median / p95 |
|---|---:|---:|
| home | 0.746 / 0.893 | 3.030 / 6.037 |
| story | 0.538 / 0.644 | 1.750 / 2.326 |
| search | 8.163 / 8.371 | 59.801 / 117.002 |
| sitemap | 4.858 / 5.306 | 74.079 / 114.933 |
| sitemap_index | 1.879 / 2.268 | 11.850 / 18.159 |

Common-term search scans many FTS matches and contention is visible. This modest workload does not establish maximum capacity, production SLOs or a WordPress comparison. The measured executable was 11,309,296 bytes, RSS after load 63,635,456 bytes, site files 33,199,552 bytes (database/WAL and fixture data included, server log/config excluded). Platform, fixture, client overhead and actual executable hash are recorded; these are not idle-memory or final Linux-artifact numbers.
