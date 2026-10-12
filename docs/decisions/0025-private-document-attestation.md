# 0025 — Local scan admission and response document attestation

Date: 2026-10-12. D11 extends F041/F043 on merged D10; the final product scope remains unchanged.

Optional local ClamAV admission protects private uploads/downloads under a finite, fail-closed protocol. The engine/signature dataset and OS isolation remain operator dependencies. Disabled scanning stays usable and explicitly unscanned. Do not bundle a large stale intelligence dataset into the main binary or confuse a clean result with guaranteed safety.

Generate documents on demand from immutable publication/accepted-response records, and use a domain-separated manifest and detached Ed25519 server attestation. Trust an independently obtained public key, preserve keys separately and bind public key identity to multi-process configuration. A server signature proves integrity under that key; it never upgrades a typed name into verified respondent identity or a legal/qualified/PAdES signature. Certified identity/provider integration remains follow-up work requiring real credentials and standard-specific evidence.

Use escaped printable HTML as a self-contained default artifact; optional isolated owner-installed Chromium supplies actual PDF bytes that receive their own digest/attestation. No runtime Node/cloud or mandatory renderer dependency. Reject unavailable/busy/oversized/failed rendering with actionable errors. Preserve native 24 / portable 19 only if the implementation adds no persisted state/grammar; introduce explicit migration if this changes. On-demand regeneration does not promise byte-identical earlier renderer metadata; the recipient retains downloaded artifacts.
