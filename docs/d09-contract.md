# D09 — Editorial insights and durable public analysis

Date: 2026-10-12. Active after D08 PR #24, actual-main9931dee6ae89f758a4a06691b1127a56f1d551ec, seven cumulative gates plus release, and independently verified separate CI/public archives. [D08 delivery receipt](evidence/d08-main-release.json). Scope F031/F034. No external account/key is required for native work.

## End-user outcome

Editors review useful text and focus-phrase observations through their existing content/editorial workflow, with actionable links back to editing. Results distinguish the saved working copy from actual public publication and show their source, algorithm, language boundaries and omissions. Owners run a larger bounded public graph in durable background batches, see progress, cancel/retry/remove it, and understand whether it is current and complete. Native forms, JSON and stopped-owner CLI share primitives. No ranking grade, universal readability score, preferred length or density target.

## Functional sequence

1. Establish declared Unicode text observations and bounded normalized focus-phrase locations in actual server-rendered main text, title/description, headings, image alternatives and link labels. Reuse D08 parser allocation and public route authority. Exclude inert/explicitly hidden/navigation/footer text; declare body fallback and CSS/client-layout omissions. Preserve meaningful combining characters/contractions and refuse excess work.
2. Deliver a calm native editorial workspace and per-publication view, integrated with existing editing and review. Clearly label the analyzed copy and preserve invalid input. Private working-copy observations require current editor permission and never enter the public graph; public observations refuse private/unpublished sources.
3. Add owner-started bounded durable analysis: declared public sources, finite batches, compare-and-swap ownership with expiring claim tokens, restart/resume, progress, cancellation, failure/retry and exact source-state completion. No partial/stale/overflow orphan conclusion. A worker never holds a database transaction while invoking renderer queries, preserving one-connection operation. No network crawler.
4. Harden and optimize the connected system: real SQLite/PostgreSQL source and permission races, independent workers, restart/failure/recovery, actual larger populated graph, query counts/plans, storage/RSS/latency observations and responsive/native usability. Execute scoped behavior from pinned actual free Yoast; source inspection/saved scores alone are not execution evidence. Preserve every cumulative gate and independently verified delivery.

## Source and reuse authority

A durable database epoch changes in the same transaction as relevant public rendering inputs. General request/cache generations are unsuitable because empty worker cycles and unrelated activity advance them. Precisely inventory published payload/status, site/discovery/models, live theme/shared options, redirect policy, media access/metadata, membership restrictions, taxonomy relations/names, approved comments and public consent policy. No-op operational cycles and working-only edits do not invalidate public analysis. Test rollback and direct controlled database writers. Extend the inventory when future renderers acquire a new source.

A new source epoch requires current rendering and current target resolution. Reuse only parser/text facts whose actual newly rendered HTML digest is identical to retained previous output. Never reuse old permissions, publication identity or resolved destinations. This reduces parsing work for unchanged output, while still rendering each declared source. Source changes make the old run stale; retry declares a fresh source state. Stale views expose no retained source/title/link/text observations.

## Initial engineering ceilings

Establish configurable owner limits within hard finite ceilings:5,000 public publications plus at most32 language homes;100,000 edges;128MiB aggregate rendered HTML;256MiB serialized derived facts per run; at most two retained runs and one active job. Preserve D08's per-source2MiB HTML, tree/allocation and link limits. Initially constrain extracted main text to512KiB. Batches have finite source/edge/time limits; establish actual batch size and lease duration through measurement. These are engineering admission ceilings, not proven latency/peak-memory guarantees. Populate and measure supported workloads before final delivery, and record justified adjustments.

## Version, privacy and recovery

Native schema23 will introduce precise invalidation and derived operational job state. Portable recovery may remainv18 if archived content grammar/table meanings are unchanged: analysis requests/results are a finite private cache, omitted from portable recovery and rebuilt after fresh restore. Document/test independent numbering. Retain the actual D08 native22 executable and prove a data-bearing stopped upgrade plus fresh old-runtime rollback. Normal runtime accepts one current grammar. Private operational text may include previously public personal data; finite retention, explicit deletion and stale-result suppression must be documented rather than claiming immediate disk erasure on every publication change.

## Guidance and evidence

Reviewed2026-10-12: [Google people-first content](https://developers.google.com/search/docs/fundamentals/creating-helpful-content), [crawlable links](https://developers.google.com/search/docs/crawling-indexing/links-crawlable), [UAX#29](https://unicode.org/reports/tr29/) and [Yoast analysis documentation](https://developer.yoast.com/features/analysis/overview/). Current UAX#29 is Unicode18/revision49. Prepared unicode-segmentation1.13.3 declares Unicode17 data; explicitly identify this supported version and default-boundary limitations rather than calling it latest or claiming dictionary segmentation. NFC/Unicode lowercase/normalized-whitespace phrase observations are a declared heuristic, not stemming, full case folding or locale-specific casing. Revisit data/algorithm versions through guidance and dependency review.

Free Yoast28.6 archive SHA-256:ebb6e2a89d697cabecf2348a06d1efe807fd13ee1faeddcd06e354b98ee2a4d5. Source pin/inspection is preparation; actual execution and premium descriptions remain separate. Maintain feature-guidance records, versioned algorithms, future review dates and mapped tests. Rich dictionary, grammatical, morphology and proprietary assessment equivalence remains explicitly scoped breadth, not implied by Unicode counts.

## Readable acceptance clusters

Verify real multilingual/combining/emoji/contraction text and complete-word phrases; native owner/editor copy selection and permission refusals; transactional source invalidation versus harmless activity; independent-worker cancellation/retry/restart/stale suppression; a populated graph above D08's1,000 ceiling with strict over-budget refusal; fresh recovery and authentic original-runtime rollback; integrated320/768/1440 geometry/scoped accessibility and actual free-reference execution. Test counts remain distinct from coverage, aesthetic judgment and full parity.
