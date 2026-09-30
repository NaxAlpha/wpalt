# M2 verification

Date: 2026-10-01. Status: final integration checks in progress; no claim of whole WordPress/plugin parity.

M1 PR #1 and preparation PR #2 are merged. The actual merged clean build was downloaded and checked: [record](clean-build-verification.json). M2 extends the same pipeline to rebuild its locked frontend and reject stale bundled output before producing the Rust artifact.

## User-facing evidence

Four meaningful new acceptance clusters run alongside eight M1 regression journeys on actual SQLite and PostgreSQL 17:

1. Typed custom models/terms, relationship and repeater authoring, shared draft/live options, parameterized cards and private related data boundaries.
2. Theme draft preview/publication, immutable stylesheet race handling, stale optimistic writes, restore-to-draft and theme switching without losing content.
3. Unsafe URLs/styles/cycles/expansion rejected, schema preflight preserves existing data, unauthorized writes rejected and fixed/bounded resolution query work.
4. Complete design/schema/options/revision backup recovery across engines and a one-off migration from the exact merged M1 schema, with meaningful working/live content, authentication, terms and restorable revisions. Reopening proves migration does not rerun.

See [readable journeys](../../tests/acceptance.rs). Local twelve-journey dual-engine suite passed, including final schema/history/privacy refinements; reruns accompany affected changes. No trivial getter/snapshot test proliferation.

The actual Chrome journey creates a parameterized component through visual controls, binds its text, inserts an instance, saves a private draft, previews at mobile width, publishes and verifies visitor output. It rejects an invalid node identifier without damaging the saved graph, creates a custom model/field and authors through its generated widget, then verifies progressive tab keyboard behavior. Existing publishing/autosave/revision/media/comment/theme/mobile journeys remain covered. No remote runtime requests or browser script errors in the successful run. Final release rerun and screenshots are recorded when complete.

## Implementation boundaries

One Rust renderer powers authenticated draft and visitor output. Theme packages cannot execute arbitrary code. Author preview also uses this engine; authenticated draft styles are tied to the exact saved revision. Definitions, bindings, IDs, URL/CSS values, graph cycles and work are bounded. SQL values remain bound. Design JSON has a 320-KiB HTTP limit distinct from image uploads; the studio loads only the selected package rather than all installed package bodies. Related content loads only published columns; private images are omitted from public rendering. Component arguments are typed. Schema writes preflight working/live values, reference targets, terms, options and installed draft/live theme dependencies. History preserves immutable styles even after old field dependencies are removed; historical restore requires current schema validation.

A publication-version keyed cache stores immutable packages only, at most 32 entries. It never caches private or rendered HTML. The warm lookup does not fetch the entire package from the database; cold lookup uses immutable publication history to avoid mixed-version styles during a publish race. Dynamic data is request-local. Only selected-template dependencies load; model collections use a composite index and related rows use bounded bulk queries. Published column rows are streamed under a shared 2 MiB budget; assembled sections/documents are bounded too.

Preview disallows scripts and forms and allows only same-origin frame ancestors; other admin pages remain unframeable. Tabs progressively enhance on the published site and remain readable without JavaScript. Native accordion/scrollable carousel avoid automatic motion. Focused keyboard/responsive tests are not a full assistive technology or WCAG audit.

## Measurements and limitations

Performance is measured on native macOS ARM release builds with 1,000 posts and 20 dynamic cards, three typed component parameters, 100 requests per endpoint/concurrency on SQLite and an isolated PostgreSQL database. Full uncompressed HTML, localhost Python client overhead, warm application; cold first request, studio asset size, RSS and site files recorded separately. Timing values are observations, not flaky correctness assertions or a production capacity guarantee. PostgreSQL engine/VM memory and database storage must be reported separately from application RSS/local site files.

The frontend is development-time Preact 11.0.0/esbuild 0.28.2; generated assets are bundled. Formatter 3.9.9 is development-only. Runtime needs no Node, PHP or package manager. CI requires locked Rust/npm dependencies, audit, compiler floor, dual databases, release CLI and real browser checks before a separate uncached Rust/locked frontend artifact build.

[Authoring and package limits](../theme-authoring.md), [migration/recovery](../operations.md), [dated guidance mappings](feature-guidance.json) and [capability scope/remaining gaps](../feature-parity.json) are authoritative companion records. Larger plugin groups remain explicitly partial where extensions, editorial policy, multilingual behavior, protected membership, arbitrary CSS/assets or marketplace/cloud services are later/outside this milestone. No new WordPress performance superiority claim is made from unequal feature sets.
