# Local consent and account data requests

2026-10-05, M7 preview. Implement required F109/F110 with explicit locally operated boundaries.

Pinned optional analytics scripts run with owner-reviewed page privileges and retain strict CSP. A full manifest digest, included in durable grants, forces renewal for changed code or declared purposes. Source reads independently require a current grant; withdrawal reloads running page code. Scripts share the declared analytics category. There is no claim of malicious-code sandboxing, legal certification, retroactive external deletion or generic third-party tag management.

Data access is proven by an existing local session plus current-password verification, with authorization rechecked at the read/write boundary. Account exports use explicit fields rather than raw table dumps or heuristic joins by name/email. Anonymous data and outside systems require verified manual owner handling. The owner records actual erasure handling and retention reasons; the workflow never automatically removes financial identity or replays delivery. Passwordless external-only identities can request manual owner handling rather than bypassing the proof.

Schema 11 adds a private, quota-bounded request queue and indexed export predicates. Archives v11 validate and retain that graph. M6 uses a dedicated offline converter; meaningful earlier preview archives need their matching executable for fresh restore, then an additive database upgrade/re-export. No legacy runtime compatibility layer.

See resilient-operations.md, acceptance.rs, consent_scripts_acceptance.cjs and operations_acceptance.cjs for actual behavior and verification. Remaining M7 clone/selective recovery and matched performance work is not removed by this decision.
