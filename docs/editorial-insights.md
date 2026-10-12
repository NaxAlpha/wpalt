# Editorial insights and public analysis

D09 native implementation; local and delivery verification are recorded separately. No account, network crawler or runtime language service is needed.

From a saved content editor choose **Editorial insights**. Choose the saved working copy (including the saved theme draft) or the live publication as an anonymous reader. Save changes first: unsaved browser edits are not the source. An optional focus phrase has one to eight default Unicode word segments, at most100 bytes before and after normalization. Invalid native forms preserve the entered phrase and copy selection. Current editors/owners can inspect their private working copy; integration credentials and reader accounts cannot. The public choice refuses unpublished or membership-protected content.

Observations count grapheme clusters, default word/sentence segments and text blocks. Phrase locations cover title, description, main text, headings, image alternatives and anchor labels. Text excludes inert, explicitly hidden, navigation and footer markup; without a main element it uses body fallback. CSS visibility, visual ordering, generated client text and embedded frame content are outside this server HTML analysis. Default boundaries are not a language dictionary. NFC/Unicode lowercase/whitespace matching does not perform stemming, grammatical evaluation, full case folding or locale-specific casing. No density target, preferred length, universal readability score or ranking promise is provided. Segmentation and NFC tables are version17.0.0; lowercase tables come from the declared Rust runtime Unicode version. Current UAX#29 guidance is version18/revision49: the supported older data version is an explicit implementation boundary.

Owners open **Public analysis** from the editorial queue or insights. Starting a run declares canonical public publications and language homes. The worker renders them in finite batches, then resolves their actual anchors/images against the declared sources and current route/privacy authority. Progress survives restart. Owners can process the next batch manually, cancel, remove or start a fresh run after a failure/source change. Expiring database claims and version comparisons prevent competing workers or late cancelled workers from replacing committed progress. Rendering runs outside database transactions, including with one connection.

A complete current run offers paginated sources, observed link issues and inbound observations. No inbound anchor means only that the completed declared graph has none; external links, query-dependent destinations, media variants, client text directives and undeclared route families remain explicitly skipped/unscanned. Partial, failed and stale runs never produce an orphan conclusion. Source changes hide all retained source text/link/phrase details. Each new run still renders current output; it reuses previous parsing facts only for an identical actual HTML digest under the same analysis/render identity. It never trusts old access decisions or resolved destinations.

## Configuration and stopped-host CLI

Add to a TOML configuration (defaults shown):

```toml
[analysis]
enabled = true
max_publications = 5000
max_edges = 100000
max_html_bytes = 134217728
max_facts_bytes = 268435456
batch_sources = 8
batch_seconds = 5
lease_seconds = 120
retention_seconds = 604800
```

These are finite engineering admission limits, not latency, peak-memory or throughput guarantees. Hard ceilings match the publication/edge/byte defaults; batch size can be1..32, duration1..10 seconds, lease30..600 seconds, retention60 seconds..30 days. At most32 language homes and two retained runs are admitted. Each source preserves the existing2MiB HTML and parser allocation/tree/link ceilings; extracted main text is at most512KiB, labels2MiB. A deadline or aggregate refusal produces a failed run rather than partial conclusions. Disabling background analysis removes its worker work and still permits removing retained private runs. Individual editor observations remain available.

```sh
wpalt --config site.toml editorial-analysis start --focus 'garden guide'
wpalt --config site.toml editorial-analysis tick
wpalt --config site.toml editorial-analysis status
wpalt --config site.toml editorial-analysis sources RUN_ID --after -1
wpalt --config site.toml editorial-analysis source RUN_ID SOURCE_ORDINAL
wpalt --config site.toml editorial-analysis cancel RUN_ID CURRENT_VERSION
wpalt --config site.toml editorial-analysis remove RUN_ID CURRENT_VERSION
```

The stopped-host lock applies. The regular worker advances runs automatically. Native forms and owner JSON endpoints under `/api/admin/analysis` use the same primitives; writes require current owner permission, same-origin controls and CSRF. Editor POST `/api/admin/insights/CONTENT_ID` accepts`csrf`, `copy` (`working`/`public`) and optional`focus`. Observations are private administrative output.

## Privacy, migration and recovery

Native schema23 adds durable source invalidation and derived operational tables. Portable content grammar remainsv18: the new tables are not archived content and are deliberately excluded from portable recovery. Fresh restore rebuilds observations. Authentic retained native22 upgrade/fresh old-runtime rollback passes locally on both databases; exact-source Linux delivery verification remains required; normal startup does not silently upgrade an adopted site.

The source epoch tracks relevant published snapshots, live design, site/models/discovery, current redirects, public media metadata/access, membership restrictions, referenced taxonomy and approved comments. Working-only edits, unused draft terms, pending comments, 404 aggregates and empty publication cycles do not change it. Trigger updates and source writes roll back together. Configuration/algorithm identity also fences results. Future public render dependencies must extend this inventory and its connected regression.

Previously public personal text can remain in the private derived cache until explicit removal or finite expiry; stale suppression is not immediate disk erasure. The running worker performs expiry. Starting a third run removes the oldest previous run, including its page/inbound data. Database allocation/WAL and owner-native backups may retain deleted bytes under their normal storage lifecycle. Offline/disconnected workers cannot perform timed deletion until restarted. Protect database/data-directory access as for other private administrative state.

Reviewed2026-10-12: [Google people-first guidance](https://developers.google.com/search/docs/fundamentals/creating-helpful-content), [crawlable links](https://developers.google.com/search/docs/crawling-indexing/links-crawlable), [Unicode segmentation](https://unicode.org/reports/tr29/), [Yoast analysis overview](https://developer.yoast.com/features/analysis/overview/). Actual free-plugin execution, implementation limits and measured runtime evidence remain separate from vendor capability descriptions and normative guidance.
