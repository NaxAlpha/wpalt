# Cumulative M1–M4 adversarial review

Authorized 2026-10-03, before M5. Baseline: merged M4 `91cbe6c7fbf1d34f1e7f4ba475da22f45dcf9436`. This supporting review does not add a product milestone or claim future functionality. Deliver fixes and evidence in a dedicated PR; merge requires the user's review.

## Completion protocol

1. Inventory every implemented capability, route, configuration boundary, shared primitive and existing guarantee. Review implementation and tests independently of previous completion claims. Record feature-level evidence and gaps.
2. Attempt failures in isolation: malformed/oversized input, missing permissions, stale state, invalid configuration, private/public leakage, database/storage/transport errors and browser interaction failures. Add focused readable regressions for confirmed failures.
3. Review integrated publishing/design/discovery, forms/audience/mail/engagement, access/revocation, migrations/backup/fresh recovery and module-disabled journeys. Exercise real SQLite and PostgreSQL, independent application instances and controlled races.
4. Review populated and failure UI states, keyboard/focus, 320/768/1440 layouts, long content, text spacing, reduced motion, design tokens and actual screenshots. Measure representative request/query/resource costs with stated conditions. Refresh authoritative security/accessibility/SEO/database guidance when relevant.
5. Repeat affected adversarial probes after fixes, then run cumulative regression gates, current-head CI and exact-source clean-artifact verification. Reconcile coverage, findings and limitations. Passing automation alone is not comprehensive security or aesthetic certification.

Each round records source inspected, attack/probe, observed result, severity, remediation and verification. No feature is covered merely by listing its file; no scanner/test count substitutes for review. Future-milestone functionality stays explicitly outside current scope. Known supported-deployment limitations are distinguished from defects in current guarantees.

## Review ledger

In progress. Findings are provisional until reproduced. The coverage ledger will distinguish reviewed, verified, limited and pending capabilities; review is not complete while required coverage or material fixes remain pending.

## Round 1 — initial trust-boundary findings

| ID | Finding | Severity / state | Evidence / remedy |
|---|---|---|---|
| A01 | `/account` renders identity and a CSRF value without `no-store`; logout also lacked an explicit private cache policy. | Medium; patched, focused SQLite check passes; PostgreSQL pending. | Extend private-response policy and the existing access journey. |
| A02 | Login verifies a captured password hash outside the mutation coordinator, then inserts a session without checking a concurrent password/role change. | High; patched; controlled SQLite regression passes; PostgreSQL pending. | Conditional session insertion rechecks verified hash/role; controlled verification-to-commit regression and normal new-password login. |
| A03 | Expired staged attachment cleanup could unlink a file while an admin snapshot was reading the previously captured row. | Medium; patched; focused SQLite regression passes; PostgreSQL pending. | Coordinate cleanup with the single-server snapshot lock through file removal. |
| A04 | A promotion with allocation disabled still accepts direct reward claims from an otherwise valid consented impression. | Medium; patched; focused SQLite regression passes; PostgreSQL pending. | Claim requires `wheel=1`; existing stock/privacy/recovery journey probes disabled allocation without consuming stock. |
| L01 | Last-administrator protection, publishing/schema coordination and snapshot/filesystem coordination use a process-local mutex. | Deployment boundary; broader multi-instance guarantees pending M9. | ADR 0001 and roadmap explicitly limit current operation to local/single server. Existing DB-backed M4 races remain required; do not imply full distributed support. |
| D01 | Authoring operations still referred to intermediate M4 backup v6. | Documentation defect; corrected to merged v7. | Schema and actual archive envelope are 7. |

Guidance refreshed 2026-10-03: [OWASP session management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html), [web cache security](https://cheatsheetseries.owasp.org/cheatsheets/Web_Cache_Security_Cheat_Sheet.html), [PostgreSQL 17 locking](https://www.postgresql.org/docs/17/explicit-locking.html), [W3C reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html), [focus not obscured](https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html). These sources inform the probes; their existence is not proof that all applicable behavior passes.

### Additional isolated findings

| ID | Finding | State / verification |
|---|---|---|
| A05 | Content could use system-route slugs, including `/account`, and become unreachable. | Reserved namespaces extended; configuration regression passes locally. Restore-boundary coverage remains pending. |
| A06 | Promotion requests loaded both large document variants for every active promotion and made per-candidate impression queries. | Metadata joined in one query; only the selected document is loaded. Behavioral journey passes; populated performance measurement pending. |
| A07 | Restored password hashes were syntactically accepted with attacker-selected excessive Argon2 work factors. | Bound algorithm, version, encoded length and memory/time/parallelism before restore and login. Malicious archive regression passes locally. |
| A08 | Inline groups and reusable object groups validated successfully but were inconsistently resolved by theme bindings. | Shared child-field resolution; integrated publishing regression passes locally. |
| A09 | Whole-gallery text bindings could serialize private media identifiers. | Recursive public projection removes private references; public gallery and ordinary identifier-shaped text regression passes locally. |
| A10 | Configured redirects could target private account and consent/registration proof routes. | Extend operational-path exclusions; validation regression passes locally. |
| A11 | Accepted uppercase HTTPS origins retained a noncanonical origin and failed to enable secure cookies. | Derive origin and HTTPS scheme from the parsed URL; configuration regression passes locally. |

Round 1 local cumulative verification: `cargo test --locked` passes, including SMTP delivery/lost-receipt recovery on an authorized loopback listener. This is SQLite evidence; PostgreSQL evidence remains pending. Negative controls for A01–A04 deliberately restore the failing behavior and demonstrate that the corresponding probes fail. The full review is still in progress: browser, populated measurements, remaining feature coverage and final clean-artifact gates are required.

## Round 2 — integrated browser and resource boundaries

The release checkpoint passed the cumulative real-Chrome journey, including the linked form, engagement, workflow and authoring probes. Eleven core admin routes were additionally measured at 320/768/1440 widths (studio breakpoint edges also covered) under user text-spacing overrides. The stylesheet is a test-only same-origin fixture; production CSP remains enforced. The narrow content editor screenshot was visually inspected for readable controls, hierarchy and unclipped layout. This is representative visual evidence, not complete aesthetic certification.

A05 restore follow-through: an archive with a recomputed checksum and reserved published slug must fail before recovery. A12: media responses and backup/private attachment reads previously trusted unbounded file reads or a separate metadata check; the read itself now has a finite byte cap, including the remaining backup budget. A corrupted 33 MiB file regression verifies rejection and subsequent successful recovery after repairing the file.

The access journey now enumerates administrative route patterns and probes anonymous and subscriber GET requests. Data lookup must not substitute for authorization: only authentication/authorization denial or method rejection passes. Existing valid-write probes remain necessary; a malformed write does not establish permission enforcement.

Promotion edits intentionally renew consent under the documented purpose/catalog/promotion policy; this behavior is retained. Multi-process publishing/authentication/file coordination remains the explicit M9 boundary, with current single-site process ownership enforced by the operator lock.

Checkpoint performance measurement: native macOS arm64 release, 1,000 stories, composed twenty-card home, discovery endpoints, 200 requests per scenario, concurrency 1/10. Ten scenarios measured; client overhead and concurrent browser/test work are included. The measured executable SHA is recorded in the raw result; these numbers cannot be attributed to later source revisions without rerunning. Final representative performance and PostgreSQL evidence remain pending.

## Round 3 — stronger probes and negative controls

A13: content export previously fetched up to 10,001 complete rich content rows before checking the count, without an aggregate byte budget. Export now consumes rows incrementally, preserves the existing JSON format and returns a complete result or a byte/count-limit error. The connected regression exercises rejection, unchanged source data and successful complete retry with a larger budget on each configured engine.

The populated browser extension discovers actual owner detail links after form/audience/mail/engagement workflows. It passed 15 business screens at 320/768/1440, normal and user text spacing, with narrow-screen axe checks and screenshots. Together with the eleven core screens this covers both the component foundations and populated cross-feature administration. Private response, contact, campaign, delivery and promotion detail screens are included; no download or mutation is performed by the screen sweep.

Three further negative controls remove restored-hash bounds, recursive private-reference projection and export byte limits in turn. Each corresponding behavioral assertion fails, and source is restored after each probe. Seven negative controls now distinguish meaningful regression protection from tests that merely repeat the implementation. Additional anonymous/subscriber inventory probes include `/api/admin` reads.

Refreshed implementation guidance: [WHATWG URL origin rules](https://url.spec.whatwg.org/#concept-url-origin) and [version-pinned Argon2 parameter definitions](https://docs.rs/argon2/0.5.3/argon2/struct.Params.html). The generated password policy remains Argon2id v19 with library defaults; imported hashes must stay within the documented resource envelope (at most 64 MiB memory, ten passes, four lanes, 512 encoded bytes). These are defensive import ceilings, not recommendations to weaken new passwords.

### Measurement correction and cumulative scope

The queue-plan probe previously bound yesterday's cutoff, measuring an empty ready range despite 1,000 populated jobs. It now asserts 1,000 currently due jobs, refreshes planner statistics after fixture population, and records the actual cutoff/statistics conditions. This corrects verification quality without changing the delivery engine.

The coverage ledger maps 49 implemented capability groups to their defined scopes and associated isolated/integrated journeys, alongside 101 route patterns and 21 shared boundaries. It records that cluster evidence is not exhaustive permutation coverage. Source review includes all route-owning modules and their role/CSRF guards, transactional recovery and migration stages, design/authoring state, database query shapes and privacy-safe diagnostics.

SEO guidance refreshed against Google's [canonical guidance](https://developers.google.com/search/docs/crawling-indexing/consolidate-duplicate-urls), [sitemap overview](https://developers.google.com/search/docs/crawling-indexing/sitemaps/overview) and [localized versions](https://developers.google.com/search/docs/specialty/international/localized-versions). Existing probes check absolute local canonical/metadata, publication-only sitemap/indexing and reciprocal published language variants. The declared minimal schema graph, exact redirects and local link checker remain their defined scopes; richer future SEO functionality is not claimed.

Checkpoint `811af0d` passed the application job with real SQLite/PostgreSQL 17 and Linux Chromium, plus the compiler floor. Later export/UI/measurement changes require their own final-head gates. Full local Rust checks and negative controls remain distinct evidence from CI. No user-visible milestone scope changed.

A12 follow-through also bounds reads of pre-existing mail spool files by the durable expected message size. A corrupted file becomes an explicit retry state; repairing the file allows stable-byte spooling to resume. The consent/mail/recovery journey verifies this without contacting an external recipient. Concurrent outbox collisions remove the temporary file even when the existing file fails its size/integrity check.

Migration note: previously accepted content using a newly reserved system slug must be renamed and republished before exporting a current recoverable snapshot. For an old archive, use its matching runtime to restore into an isolated site, correct the slug, then create a new snapshot. The review does not silently rename content or add a legacy parser.

A14 (UX): editor navigation advertised Engagement reports even though their route requires an administrator. The shared navigation now follows the same owner-only policy. The access journey verifies that the link is absent for an editor and remains available to an owner; authority is still enforced by the backend.

A15 (functional/access): login-page redirection always sent authenticated users to administration, while administration redirected subscribers back to login. An approved subscriber visiting login could therefore enter a redirect loop. Login now returns subscribers to `/account`; administration returns a permission denial instead of redirecting a forbidden session. The access journey and actual registration/login browser workflow cover this role transition. Database errors in dashboard authentication also propagate as errors rather than masquerading as a sign-in redirect.

A16 (recovery): account archive validation did not preserve the runtime's last-administrator and lowercase-login invariants. A syntactically valid rewritten archive could restore with no administrator, or store an address that normalized login cannot find. Recovery now rejects both states before insertion; the backup journey verifies rejection and subsequent successful original-archive recovery. No account is silently promoted or renamed.

A06 measurement follow-through: the one/100-candidate release probe confirmed seven correlated queries in both cases, but exposed SQLite overflow-page work despite the narrow SELECT projection. SQLite now has covering targeting/listing indexes over bounded metadata; PostgreSQL uses thin ordering/filter indexes and keeps large target JSON out of B-tree tuples. Indexes are derived, created idempotently at startup, and do not require an archive-format migration. The reproducible benchmark records actual plans, payload bytes, latency samples and observed RSS; final rerun remains required.
