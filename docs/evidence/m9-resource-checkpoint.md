# M9 local-process resource checkpoint

Measured 2026-10-05 on macOS-26.6.2-arm64-arm-64bit-Mach-O. The optimized executable at source checkpoint `8a9e7fd` has SHA-256 `68815cd800eb68e7f582d9792bf06747a6e07b3b9f0638238e44204cb7f2374b` and 28167520 bytes (26.86 MiB). This is a macOS native executable, not the Linux clean-build artifact. Later diagnostics changes require final affected remeasurement before M9 completion.

Each executable/topology set uses a fresh SQLite site and isolated PostgreSQL schema, 1,000 seeded posts, 20 reusable parameterized home compositions, four commerce products and one bookable slot. Warm public home/story/search/sitemap/index plus uncached catalog/product documents; cache enabled, debug/SQL logging enabled, 100 successful requests per scenario, three repeats at concurrency 1/10/32. Measurements run sequentially after local builds/tests finish. Python localhost HTTP overhead is included; full uncompressed documents, no browser asset/render timing. Two-node requests alternate direct endpoints with the same configured public origin, avoiding proxy overhead; correctness through the round-robin origin is separate browser evidence.

126 scenarios / 12,600 successful requests per set, 37,800 across the three sets. No failed HTTP request was accepted: a non-200/transport error fails the workload. Native read-only snapshot optimization was verified separately by unchanged durable-state modification time and cross-node authority/replay/crash tests. The pre-optimization development executable is retained by hash; its working-tree source was not a separately published release, so it is not claimed as an independently reproduced baseline.

## PostgreSQL response p95

Median p95 across the three runs, milliseconds. Single-process and coordinated two-process modes have different safety/admission costs; this is not a horizontal scaling claim.

| Endpoint | Before redundant sync removal, c10 | Single process, c10 | Two processes, c10 | Two processes, c32 |
|---|---:|---:|---:|---:|
| home | 467.3 | 1.99 | 31.87 | 55.97 |
| story | 490.53 | 1.96 | 18.58 | 57.81 |
| search | 433.07 | 1.87 | 24.88 | 54.91 |
| catalog | 528.81 | 2.44 | 30.8 | 67.86 |
| product | 523.17 | 2.94 | 45.35 | 75.69 |
| sitemap | 502.0 | 2.82 | 20.16 | 57.18 |
| sitemap_index | 449.22 | 2.75 | 25.03 | 54.47 |

The conservative coordinator serializes requests/cycles. Removing unchanged-state sync substantially reduces its measured storage cost, but two processes remain slower than one at these read-heavy workloads. Queue fairness/lock polling and changed-state durability remain additional optimization/failure obligations. Baseline-derived budgets for the next affected remeasurement: all 100-request scenarios complete with zero failures; repeated two-node public/commerce p95 <100 ms at concurrency 10 and <150 ms at concurrency 32 on this host/workload. These are regression budgets for these conditions, not universal production promises. First establish evidence on the release runner before transferring a numeric budget to a different platform.

## Footprint and query evidence

| Mode | Application/worker RSS after load, MiB | Persistent site files, MiB | PostgreSQL schema, MiB |
|---|---|---:|---:|
| single-process / sqlite | 50.0 | 19.75 | — |
| single-process / postgres | 39.7 | 0.0 | 13.74 |
| local-processes / postgres | 38.8, 38.7, 32.9 | 0.0 | 13.69 |

RSS values are resident snapshots, not peaks; OS compression/memory pressure can change them, so no before/after RSS improvement is claimed. Two-process values list node A, node B and the independent worker. Shared PostgreSQL engine memory, cluster catalog space, WAL and server logs are excluded and must be budgeted separately. SQLite persistent files include the database/WAL plus native site data; PostgreSQL schema bytes include table indexes/TOAST, while private site bytes are separate. Synthetic configs/runtime logs are excluded from persistent-site totals but log bytes are recorded in raw evidence.

Debug records demonstrate actual correlated native SQL counts; eligible cached public responses legitimately perform zero native SQL. Each coordinated HTTP request additionally checks schema/held authority with one indexed-row query outside the inner request span, explicitly added to total query counts. Engine query plans are recorded on the populated fixture; planner-selected scans for broad sitemap enumeration are evidence, not automatically defects or an excuse to force an index. Raw evidence remains under `work/m9-resource-*.json`; final delivery must preserve safe exact-source results in the review packet and independently verify the release executable.

Executable before-state-write-optimization: `e85ba32296659e3d2b8215334e8a3e71ade015d97d1e880ce411c9dcafda983c`.
Executable single-process: `68815cd800eb68e7f582d9792bf06747a6e07b3b9f0638238e44204cb7f2374b`.
Executable local-processes: `68815cd800eb68e7f582d9792bf06747a6e07b3b9f0638238e44204cb7f2374b`.
