# M9 final-source resource verification (active)

Implementation checkpoint `c222143f956c1b7367b40771099cce480d177711`; macOS optimized executable 28,367,104 bytes (27.05 MiB), SHA-256 `2a7a09ee8a9b5d6f3423cab5e3543710c17dc8ff70f9ebfcce680509aef54e35`. Later contention refinement is under verification and must replace the final executable identity before readiness.

Same host and fixtures as [the baseline](m9-resource-checkpoint.md): 1,000 seeded posts plus the seeded home, 20 reusable compositions, commerce fixtures, real SQLite/PostgreSQL, 100 requests per scenario, three repeats, concurrency 1/10/32, cache and redacted debug SQL enabled. No model inference or competing build ran during measurement.

Initial final-source single/two-process workloads completed 25,200 successful requests. Two-node home c10 repeat 1 p95 was **113.365 ms**, exceeding the existing 100 ms per-scenario budget. Review found warm-up visited only node A. Corrected the benchmark to warm each independently cached node while preserving separately measured cold start; the additional 12,600-request run still had home c10 repeat 1 **100.965 ms**. Neither failure is waived or erased; reducing bounded lock-poll delay is currently being tested.

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
