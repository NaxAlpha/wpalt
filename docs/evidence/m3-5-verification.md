# M3.5 verification record

2026-10-02. [Delivery PR #7](https://github.com/NaxAlpha/wpalt/pull/7) is the review surface. Accept delivery only after its current-head application, Rust 1.85 compiler-floor and independent clean-build jobs pass and the downloaded archive matches the tested merge snapshot. Review/merge remains a separate user decision. This is the approved integrated-authoring milestone, not full WordPress/Notion parity or a production-readiness certification.

## User-visible result and data integrity

One canonical versioned tree connects the direct editor, safe public rendering, working/live snapshots, optimistic saves, schedules, revisions, backups and explicit Markdown imports. The bundled locked ProseMirror engine supplies contextual formatting, slash/block insertion, basic tables, uploaded-image selection, undo/redo, keyboard and pointer organization. Language/SEO and typed fields use disclosures. No runtime Node, external account, font or authoring service is required.

Native typing before enhancement loads is preserved; the page keeps its native import form. Existing canonical documents require explicit Markdown replacement in that fallback. Modal actions synchronize pending native caret selection through public engine APIs, and Enter applies the intended link/image action. Document-start/end shortcuts use engine selection transactions; the browser cluster rapidly navigates immediately after composition and observes the caret before formatting to guard against delayed native-selection reconciliation. Browser recovery explicitly restores the document and metadata, including unchecked boolean controls; it does not silently overwrite server content. Competing saves preserve text and report a version conflict.

Schema 4 migrates draft/live source and retained history inside one transaction. Unsupported input aborts with source/version intact; the correction/retry cluster verifies this. A save writes content, metadata and snapshots in one optimistic INSERT/UPDATE rather than rewriting the wide row twice. Public routes load only published document columns. The protected image list is permission-checked and keyset-paged at 40 items, without fetching image bytes. Backup v4 and content export v2 retain canonical trees. Markdown projections are readable but lossy for rich structures; full JSON backups are authoritative. See [operations and migration](../authoring.md) and [ADR 0005](../decisions/0005-structured-editor.md).

## Meaningful verification

- Local warnings-denied Clippy, Rust format and locked frontend reproducibility/format checks pass. **19 high-level SQLite acceptance clusters and two document import/trust clusters** pass. Existing publication, discovery, permissions, scheduling, media and recovery journeys remain active. The discovery-volume fixture clones canonical documents as well as projections and renders a sampled post to guard against empty legacy-only test rows. Required CI executes the same database journeys against real PostgreSQL 17; no mock substitutes for that engine.
- Real Chrome 154 verifies early input before enhancement; native cursor/selection and preserved source; formatting/link modal Enter; slash blocks; table operations; actual uploaded-image selection; changed keyboard/drag block order; duplicate/undo/redo; sanitized clipboard payload; publication/reload; failed saves; explicit document/metadata/unchecked-flag recovery; and stale writers. It also exercises Japanese/Arabic text and a **synthetic composition event/DOM boundary**. This does not certify a native OS IME.
- The shared UI suite passes **38 geometry samples, 12 accessibility scans and three independent component-gallery comparisons**. The authoring canvas additionally measures 320/1440-pixel viewports: 18-pixel text, 31.5-pixel line height, 288/752-pixel writing width, no page overflow. [Desktop](m3-5/authoring-desktop.png) and [mobile](m3-5/authoring-mobile.png) screenshots were visually reviewed: calm cardless layout, stable typography, bounded table geometry, writing before disclosed metadata, and wrapped controls. The tiny image is deliberately the test upload, not placeholder artwork.
- The 1,000-paragraph, 74,888-byte document observation is **81 ms load and 5 ms single-input response** on this local run. These are descriptive measurements, not timing assertions or cross-machine guarantees. [Authoring results](m3-5/authoring.json), [UI summary](m3-5/ui-summary.json).
- Fresh release CLI installation/configuration precedence, process locks, schedule/restart, RSS XML, portable themes, private backups/fresh restore and redacted debug logging pass. Debug records document size/node counts/validation duration, not private draft text. Server grammar bounds nodes/depth/text/JSON/table dimensions and rejects unsafe URLs/attributes before storage. Paste payloads do not execute; browser journeys make no external requests or script errors.
- Advisory review reports **zero selected Rust vulnerabilities/warnings and zero npm graph vulnerabilities**. One inactive optional Rust lockfile RSA advisory is distinguished using the actual selected all-target feature graph, without a blanket ignore. [Dependency evidence](m3-5/dependency-audit.json). Nineteen feature-maintenance records pass the freshness checker; current engine/input/SQL primary references are mapped to the authoring record.

## Footprint and optimization

The bundled editor is **274,566 bytes raw / 82,213 bytes gzip** with dependency notices. It is embedded in the binary and loaded on authoring pages. The runtime remains a single Rust executable.

The release benchmark uses an isolated **3,000-post SQLite blog**, 30 requests per endpoint/concurrency setting, full uncompressed HTML, warm localhost requests and no response cache; Python client overhead is included. Binary identity, machine, observed RSS/running site files and individual p50/p95/p99 results are in [release measurements](m3-5/release-performance.json). These observations are not capacity promises or a new WordPress comparison.

Public listing/search ranks at most 21 lightweight candidate identities in a materialized stage before loading text/fields. SQLite's compact partial identity index avoids reading wide post rows for SEO filtering during search; PostgreSQL retains its GIN index and the bounded candidate stage. [Actual builder query-plan capture](m3-5/query-plans.json) uses production query literals on a release-seeded database: home uses the existing language index; search uses FTS plus the partial identity index. Search may still sort matching identities; this is bounded output, not a claim that full-text candidate work is constant-time. We removed a second proposed partial index after the actual home plan preferred the existing index, avoiding unproven storage/write overhead.

The storage increase from canonical working/live documents and retained histories is reported directly alongside the M3 baseline below. A single wide-row write reduces unnecessary rewrites; it does not eliminate the deliberate document/history cost. Site files include the running SQLite database/WAL and data, exclude binary/config/logs, and are not a minimal empty installation.

## Boundaries

Basic tables exclude merged cells and arbitrary widths. Imported remote images are never fetched server-side and remain blocked by the self-only image policy; chooser/paste uses local media. Clipboard input is programmatically supplied in the test. Browser storage is explicit local recovery convenience, not offline synchronization, guaranteed crash recovery or a substitute for independent backups. Native OS IME, screen readers and broader device/browser release certification remain M9 work. Real-time coediting, Notion databases/workspaces and automatic translation remain outside this authoring contract.

## Measured release comparison

| Measure | M3 baseline | M3.5 |
| --- | ---: | ---: |
| Executable bytes | 11,309,296 | 11,706,000 |
| Observed RSS bytes | 63,635,456 | 57,950,208 |
| Running site-file bytes | 33,199,552 | 47,821,744 |
| home p95, concurrency 1 (ms) | 0.893 | 1.452 |
| home p95, concurrency 10 (ms) | 6.037 | 3.867 |
| story p95, concurrency 1 (ms) | 0.644 | 0.598 |
| story p95, concurrency 10 (ms) | 2.326 | 2.432 |
| search p95, concurrency 1 (ms) | 8.371 | 5.289 |
| search p95, concurrency 10 (ms) | 117.002 | 47.821 |
| sitemap p95, concurrency 1 (ms) | 5.306 | 7.58 |
| sitemap p95, concurrency 10 (ms) | 114.933 | 131.65 |
| sitemap_index p95, concurrency 1 (ms) | 2.268 | 1.936 |
| sitemap_index p95, concurrency 10 (ms) | 18.159 | 14.413 |

These short runs include operating-system/client noise. Do not infer a guaranteed speedup, memory ceiling or regression threshold from them. The storage increase is real in this fixture; structured snapshots/history and text projections consume space. No broad WordPress-relative footprint claim is made.

## Approved merge and actual-main artifact

PR [#7](https://github.com/NaxAlpha/wpalt/pull/7) merged as `df8208f64c65853ec1cdabdef6a13ca1186805b1`. All three jobs in [actual-main run 37000435046](https://github.com/NaxAlpha/wpalt/actions/runs/37000435046) passed. Downloaded its clean Linux artifact and verified executable mode, archive/binary hashes, source revision and Rust/frontend locks, editor/Studio/CSS/token hashes against that merged revision. Archive SHA-256: `108274a5175bf86875b93c0b3b0379443082e7c09fae3199847eb64ffb487ad3`. This verifies the merged build separately from PR checks.
