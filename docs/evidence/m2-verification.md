# M2 verification

Date: 2026-10-01. Status: local release verification passed; final PR-head CI pending; no claim of whole WordPress/plugin parity.

M1 PR #1 and preparation PR #2 are merged. The actual merged clean build was downloaded and checked: [record](clean-build-verification.json). M2 extends the same pipeline to rebuild its locked frontend and reject stale bundled output before producing the Rust artifact.

## User-facing evidence

Four meaningful new acceptance clusters run alongside eight M1 regression journeys on actual SQLite and PostgreSQL 17:

1. Typed custom models/terms, relationship and repeater authoring, shared draft/live options, parameterized cards and private related data boundaries.
2. Theme draft preview/publication, immutable stylesheet race handling, stale optimistic writes, restore-to-draft and theme switching without losing content.
3. Unsafe URLs/styles/cycles/expansion rejected, schema preflight preserves existing data, unauthorized writes rejected and fixed/bounded resolution query work.
4. Complete design/schema/options/revision backup recovery across engines and a one-off migration from the exact merged M1 schema, with meaningful working/live content, authentication, terms and restorable revisions. Reopening proves migration does not rerun.

See [readable journeys](../../tests/acceptance.rs). Local twelve-journey dual-engine suite passed, including final schema/history/privacy refinements; reruns accompany affected changes. No trivial getter/snapshot test proliferation.

The actual Chrome journey creates a parameterized component through visual controls, binds its text, inserts an instance, saves a private draft, previews at mobile width, publishes and verifies visitor output. It rejects an invalid node identifier without damaging the saved graph, creates a custom model/field and authors through its generated widget, then verifies progressive tab keyboard behavior. Existing publishing/autosave/revision/media/comment/theme/mobile journeys remain covered. No remote runtime requests or browser script errors in the successful run. The final native release rerun passed: [browser result](m2/result.json), [mobile draft studio](m2/m2-studio-mobile-preview.png), [published components](m2/m2-public-components.png). Release CLI install/configuration/process lock/scheduler/restart/theme package/recovery/debug-redaction checks also passed. The twelve dual-engine acceptance journeys passed in 8.87 seconds; formatting and Clippy with warnings denied passed.

## Implementation boundaries

One Rust renderer powers authenticated draft and visitor output. Theme packages cannot execute arbitrary code. Author preview also uses this engine; authenticated draft styles are tied to the exact saved revision. Definitions, bindings, IDs, URL/CSS values, graph cycles and work are bounded. SQL values remain bound. Design JSON has a 320-KiB HTTP limit distinct from image uploads; the studio loads only the selected package rather than all installed package bodies. Related content loads only published columns; private images are omitted from public rendering. Component arguments are typed. Schema writes preflight working/live values, reference targets, terms, options and installed draft/live theme dependencies. History preserves immutable styles even after old field dependencies are removed; historical restore requires current schema validation.

A publication-version keyed cache stores immutable packages only, at most 32 entries. It never caches private or rendered HTML. The warm lookup does not fetch the entire package from the database; cold lookup uses immutable publication history to avoid mixed-version styles during a publish race. Dynamic data is request-local. Only selected-template dependencies load; model collections use a composite index and related rows use bounded bulk queries. Published column rows are streamed under a shared 2 MiB budget; assembled sections/documents are bounded too.

Preview disallows scripts and forms and allows only same-origin frame ancestors; other admin pages remain unframeable. Tabs progressively enhance on the published site and remain readable without JavaScript. Native accordion/scrollable carousel avoid automatic motion. Focused keyboard/responsive tests are not a full assistive technology or WCAG audit.

## Measurements and limitations

Performance is measured on native macOS ARM release builds with 1,000 posts and 20 dynamic cards, three typed component parameters, 100 requests per endpoint/concurrency on SQLite and an isolated PostgreSQL database. Full uncompressed HTML, localhost Python client overhead, warm application; cold first request, studio asset size, RSS and site files recorded separately. Timing values are observations, not flaky correctness assertions or a production capacity guarantee. PostgreSQL engine/VM memory and database storage must be reported separately from application RSS/local site files.

The frontend is development-time Preact 11.0.0/esbuild 0.28.2; generated assets are bundled. Formatter 3.9.9 is development-only. Runtime needs no Node, PHP or package manager. CI requires locked Rust/npm dependencies, audit, compiler floor, dual databases, release CLI and real browser checks before a separate uncached Rust/locked frontend artifact build.

[Authoring and package limits](../theme-authoring.md), [migration/recovery](../operations.md), [dated guidance mappings](feature-guidance.json) and [capability scope/remaining gaps](../feature-parity.json) are authoritative companion records. Larger plugin groups remain explicitly partial where extensions, editorial policy, multilingual behavior, protected membership, arbitrary CSS/assets or marketplace/cloud services are later/outside this milestone. No new WordPress performance superiority claim is made from unequal feature sets.

## Recorded results

[Performance data](m2-performance.json), [admin/debug SQL measurements](m2-diagnostics.json), [actual PostgreSQL plans/storage](m2-query-plan-postgres.txt), and [selected Rust dependency audit](m2-dependency-audit.json) accompany this delivery. Native release executable: 10,894,016 bytes; bundled studio: 34,894 raw / 12,478 gzip bytes. At concurrency 1 the composed home p95 was 0.977 ms SQLite / 4.636 ms PostgreSQL; cold first requests were 2.196 / 11.903 ms. At concurrency 10 home p95 was 5.492 / 20.354 ms. SQLite broad search p95 at concurrency 10 was 42.654 ms: retained as a baseline to improve, not hidden by a home-only headline.

Application RSS after load was 51,101,696 / 35,192,832 bytes; SQLite site files were 13,768,601 bytes including database/WAL. PostgreSQL local application files exclude its database; the measured disposable database was 16,922,291 bytes and its engine/VM memory was not measured. No complete deployment footprint claim follows from app RSS. Studio state read p95 was 1.503 ms with ten SQL statements; the HTML studio shell used two. Cold/warm statement variations are retained in the diagnostic record. Actual plans used public_posts for the common post collection and public_model_posts for the rare page collection. Relationship acceptance compares one versus thirty rows to prevent per-item query growth.

Regression review budgets on this same local fixture are home concurrency-1 p95 below 10 ms SQLite / 20 ms PostgreSQL, studio state below 20 ms, bundled studio below 100 KiB raw, and native executable below 20 MiB. These are review triggers requiring investigation under comparable conditions, not timing assertions or claims about other hardware.

The initial implementation commit `4ddb3f8dff0a5f6784123132a69f636f9889bf58` passed all three [GitHub jobs](https://github.com/NaxAlpha/wpalt/actions/runs/36788759309). Final refinements add atomic schema snapshots, typed literals/binding checks, ordinary UUID-string rendering and a real simultaneous-theme-writer journey. The final source head must pass the same pipeline before [PR #3](https://github.com/NaxAlpha/wpalt/pull/3) is marked ready.
