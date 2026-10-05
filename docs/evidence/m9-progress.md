# M9 development ledger

## Preparation — 2026-10-05

User authorized M8 merge and M9. PR #14 merged as `c12c6894f5c7969344a74e67cd742f971db381d5`. M8 final PR source passed all seven applicable gates in run 37290213576; archive independently verified against PR merge `fa17b50feff81fbaa3f02f0a41b7d5f5f148c690`, archive SHA-256 `40a02652c5a4aeaab4eeca72b6d130f399199013d4550e8b784d11745104994c`, executable SHA-256 `f33ef39bf93b38db50f4d9babbfa170df557286c69c3afba967e239cc13ab3b8`, 33,020,520 bytes. Actual-main build/release run 37292939425 is tracked separately and remains pending here.

Created the M9 branch from merged main. Read canonical product/methodology/roadmap and inspected actual mutation/cache, database, process lock, authentication and job ownership surfaces. The existing implementation is not safe merely by removing the data-directory lock. Defined the M9 end-user contract and coordinated failure/upgrade/resource acceptance gates; no runtime distribution implementation or verification is claimed yet. Remaining planned parity entries and broader F024 gap require explicit reconciliation before final completion.

## First module ownership surface

Added `wpalt modules`, a validated offline effective configuration/module inventory with unique domain ownership and dependency names. Engagement respects parent business admission. This is not a dynamic loader or distributed runtime claim. Strict Clippy/all targets and `scripts/module_acceptance.py --binary target/debug/wpalt` pass; the connected CLI journey sets engagement true while disabling its business parent through environment overrides, verifies disabled modules, unique ownership/dependencies, no site creation/credential output and invalid configuration rejection. Pinned encoder preparation was required after branch transition; its SHA-256 verification passed. F124 is a working slice, not fully certified final ownership resolution.

## M8 actual-main release verified

Run 37292939425 passed all six applicable core gates and release; PR-only migration reference and optional workload were correctly skipped. Published [nightly-202610050952-c12c6894f5c7](https://github.com/NaxAlpha/wpalt/releases/tag/nightly-202610050952-c12c6894f5c7). Downloaded release independently verified against merged source `c12c6894f5c7969344a74e67cd742f971db381d5`; archive SHA-256 `296946f2450e591b0a92733411e338541ce7b959b4ee591b3486bbaddf7fc3ca`; binary SHA-256 `f33ef39bf93b38db50f4d9babbfa170df557286c69c3afba967e239cc13ab3b8`, 33,020,520 bytes. M8 merge/build/release verification is complete. M9 draft PR: https://github.com/NaxAlpha/wpalt/pull/16; implementation remains active.

The initial M9-01 guidance record used local paths and failed the manifest checker, which requires authoritative HTTPS links even for internal contracts. Corrected to immutable repository source links; no checker requirement was relaxed.
