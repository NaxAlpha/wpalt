# M9 final-source resource verification

Implementation checkpoint `c222143f956c1b7367b40771099cce480d177711`; macOS optimized executable 28,367,104 bytes (27.05 MiB), SHA-256 `2a7a09ee8a9b5d6f3423cab5e3543710c17dc8ff70f9ebfcce680509aef54e35`. This retained checkpoint precedes the verified refinement below.

Same host and fixtures as [the baseline](m9-resource-checkpoint.md): 1,000 seeded posts plus the seeded home, 20 reusable compositions, commerce fixtures, real SQLite/PostgreSQL, 100 requests per scenario, three repeats, concurrency 1/10/32, cache and redacted debug SQL enabled. No model inference or competing build ran during measurement.

Initial final-source single/two-process workloads completed 25,200 successful requests. Two-node home c10 repeat 1 p95 was **113.365 ms**, exceeding the existing 100 ms per-scenario budget. Review found warm-up visited only node A. Corrected the benchmark to warm each independently cached node while preserving separately measured cold start; the additional 12,600-request run still had home c10 repeat 1 **100.965 ms**. Neither failure is waived or erased; the verified bounded lock-poll refinement follows below.

## Warmed two-process PostgreSQL checkpoint

Median of three p95 values in milliseconds; median alone does not satisfy the every-scenario gate.

| Endpoint | c10 | c32 |
|---|---:|---:|

| home | 37.767 | 67.700 |

| story | 19.662 | 67.933 |

| search | 21.575 | 65.588 |

| catalog | 29.281 | 67.588 |

| product | 48.271 | 75.322 |

| sitemap | 18.489 | 50.662 |

| sitemap_index | 24.670 | 61.380 |


## Resident and persistent snapshots


- single-process/sqlite: RSS after load [51.0] MiB; site files 20,672,593 bytes; schema 0 bytes.

- single-process/postgres: RSS after load [39.03] MiB; site files 3,233 bytes; schema 14,606,336 bytes.

- two-process/sqlite: RSS after load [48.19] MiB; site files 20,725,841 bytes; schema 0 bytes.

- two-process/postgres: RSS after load [38.98, 38.86, 33.27] MiB; site files 3,615 bytes; schema 14,409,728 bytes.


RSS is a resident snapshot, not peak. PostgreSQL engine memory/catalog/WAL/logs and optional model/runtime are excluded; application logs are measured separately and excluded from site totals. Two-node RSS includes the separate worker. SQL plans and correlated query ranges remain in raw synthetic workload evidence under `work/m9-resource-final-*.json`. Public cached hits can perform zero inner SQL; coordinated admission adds one authority query. No horizontal throughput or universal latency promise is made.

## Verified contention refinement — 2026-10-06

Implementation source `0236753bac55196996e265dd6bb1c2031eeadee7`; optimized macOS executable **28,367,104 bytes (27.05 MiB)**, SHA-256 `be97b05d26f71dbff6aa0847aef259bbae85b6aff41be64a8b2cefea3fe17ac1`. Bounded asynchronous lock retries use 1 ms rather than 5 ms; exclusive ownership, request admission cap, ten-second timeout, durable intent and explicit reconciliation remain. This reduces repeated short-lock wait penalties; no kernel fairness guarantee or spin loop is introduced.

All 12,600 additional requests succeeded. Every PostgreSQL two-node c10/c32 scenario passed the existing budgets: maximum scenario p95 **89.498 ms** at c10 (<100 ms), **72.539 ms** at c32 (<150 ms). The smaller c32 maximum in this finite run is sampling/workload variation, not a guarantee that higher concurrency improves latency. Worker remains active; request/worker crash and graceful-drain regression is tracked separately.

| Endpoint | c10 median p95, ms | c32 median p95, ms |
|---|---:|---:|
| home | 15.224 | 43.608 |
| story | 18.276 | 48.761 |
| search | 20.348 | 48.656 |
| catalog | 22.406 | 61.672 |
| product | 28.772 | 68.449 |
| sitemap | 18.202 | 58.382 |
| sitemap_index | 16.422 | 43.613 |

Final two-server/one-worker resident snapshots: [38.98, 39.02, 33.14] MiB, aggregate 111.14 MiB. Private site files 3,613 bytes; PostgreSQL schema 14,426,112 bytes. Boundaries above apply. The SQLite control also repeats against this exact executable.

The final exact executable also completed 12,600 single-process SQLite/PostgreSQL control requests, with no failures. Together with final two-process/control scenarios: **25,200 successful requests**. Native process regression against the optimized executable passed all stock/booking/form races, actual worker kills, immediate reconciled retries and graceful drain. This is correctness evidence, not a throughput benchmark.

- Final single-process sqlite: resident snapshot 49.64 MiB; private site files 20,705,265 bytes; PostgreSQL schema 0 bytes.
- Final single-process postgres: resident snapshot 32.56 MiB; private site files 3,233 bytes; PostgreSQL schema 14,524,416 bytes.
