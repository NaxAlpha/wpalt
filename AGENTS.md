# wpalt development contract

Read docs/wpalt-product-goals.md, docs/wpalt-development-methodology.md, docs/wpalt-milestones.md and the active milestone contract before changing behavior.

Deliver usable end-user systems. Make each step correct, verify and optimize it, then verify and optimize the integrated milestone. Protect security and data integrity from the first implementation. Record material direction changes and realign with the user. Avoid pre-M9 compatibility shims; document migrations or resets. Verify applicable guidance with primary sources and update evidence records. Create a delivery PR for each milestone with reproducible evidence. Never claim parity or completion without evidence. Keep secrets and reference-site private data out of this public repository.

Use cargo fmt, cargo clippy with warnings denied, and relevant behavioral tests. Validate supported database paths against real SQLite and PostgreSQL. The active resumed-package contract is docs/d01-contract.md; docs/m9-contract.md remains the cumulative deployment contract; docs/frontend-foundations.md governs frontend measurements. M1/M2 journeys remain regression coverage. Read the design/visual review protocol before frontend changes. Rebuild committed frontend assets from the locked frontend toolchain when editing the studio.
