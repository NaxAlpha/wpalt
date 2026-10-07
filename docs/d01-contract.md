# D01 — Classified content directories

Date: 2026-10-06 (Tokyo). Resumed backlog package; defined directory scope verified; exact current-head delivery/merge state is recorded on the PR. See [verification ledger](evidence/d01-verification.md) and [PR #17](https://github.com/NaxAlpha/wpalt/pull/17).

## End-user outcome

An owner builds a classified directory with typed contact/date/choice/rating fields, ordered multiple relationships and hierarchical category or custom-taxonomy archives. Editors use the existing integrated authoring interface; public visitors see only published, authorized values. Native definitions travel with local backup and recovery. No provider account or arbitrary validation code is required.

## Working steps and completion gates

1. Extend the shared field grammar with integer, email, HTTP(S) URL, calendar date, single/multiple declared choice and ordered multiple-relationship fields. Support declarative numeric and Unicode-scalar text-length bounds. Validate definitions before saving, and existing draft/live values before schema changes. Preserve bulk reference validation and rendering; no per-selection queries.
2. Store bounded child-slug → parent-slug maps in versioned model definitions. Taxonomy identities are shared across models: one model owns each nonempty hierarchy; other models sharing that taxonomy inherit it. Reject cycles, unsafe slugs, more than 128 edges per taxonomy or paths beyond eight edges. Declaring a parent does not implicitly assign it or disclose draft term names.
3. Serve published descendants through existing category/tag queries and `?taxonomy=sector&term=outdoors` archives, with the same filters on `/api/content`. Preserve language, cursor, publication, noindex and access boundaries. Keep explicit links to assigned terms. Unfiltered requests must not incur a new registry query; a filtered archive uses one registry snapshot and one bound public-content query, independent of descendant count.
4. Extend existing Studio definitions and native authoring widgets, using the current cardless design and accessible controls. Choices and relationships are ordered, unique and bounded. Owner mistakes produce understandable refusal, preserving the old definition/content. Native email input is not identity verification; URL fields do not fetch resources.
5. Verify real SQLite/PostgreSQL authoring, stale edits, incompatible schema changes, private relationships, public/draft classification, unpublication and fresh recovery. Add a populated browser journey and retain geometry/keyboard/contrast regressions. Inspect query plans on populated archives and record measurements; do not claim universal query optimality or premium-plugin parity.
6. Deliver a separate PR after strict lint, relevant connected regressions, migration and integrated browser checks. Record evidence and known limits. Keep the remaining backlog visible and begin D02 after the verified D01 delivery is reviewable.

## Upgrade contract

Native schema 16 fences older binaries from the expanded definition grammar. Stop all nodes, review `upgrade`, retain the old executable/private configuration, execute the exact plan with an encrypted pre-change recovery point, then start the new executable. Native source schemas 14 and 15 are supported through the existing transactional maintenance path. This is a format fence with no new domain table or legacy runtime branch. Existing stored definitions gain optional defaults; meaningful content is not reset.

Portable table graph remains v12 because its table/column envelope is unchanged. Expanded definitions require the new executable; older implementations reject unfamiliar definition keys/types. Rollback uses the pre-change graph and old executable into a fresh target; an old runtime must not reinterpret new feature data.

## Reference and limits

Checked 2026-10-06: [WordPress taxonomy registration](https://developer.wordpress.org/reference/functions/register_taxonomy/), [taxonomy query behavior](https://developer.wordpress.org/reference/classes/wp_tax_query/), and [ACF relationship documentation](https://www.advancedcustomfields.com/resources/relationship/). These establish reference capabilities, not a claim of directly tested paid functionality. The bounded shared schema, query URLs and constraints are wpalt design choices.

No arbitrary PHP/JavaScript validation, recursive public HTTP requests or vendor license checks. Bidirectional materialized relations, computed fields and every vendor-specific field family are not promised by this bounded package; evaluate remaining useful local families through explicit later backlog entries rather than calling this universal ACF parity. Existing authoring selectors expose the latest 128 records/media; broader searchable pickers remain a documented usability gap until implemented.
