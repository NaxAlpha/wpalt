# Published structured facts and locations

D07 adds owner-controlled typed structured output to the existing content models. It requires native schema 21 and portable recovery format v17. No wpalt account, network vocabulary lookup or search-engine account is required. This is a native structured-content system, not a complete search-engine eligibility validator or an importer for every SEO plugin.

## Create and publish

In **Studio → models**, select or add a content model. Open **Published structured facts**, select an Article, Public person, Organization, Business location or Breadcrumb starting definition, then create it. Helpers add ordinary optional typed fields; they never invent an address, author, review, price or date. Existing field-name collisions are refused rather than overwritten. Model field definitions and shared definitions are separate disclosures so everyday graph configuration remains manageable.

Review the primary entity, its known types and property bindings. Type/property search runs only when requested and uses the bundled Schema.org 30.1 vocabulary. Pending terms need explicit admission; superseded terms are refused. Save the model: existing working and published values are validated before the exact-version configuration change. Configuration changes apply immediately to publications; content changes apply when published. Removing the structured declaration keeps the underlying content fields.

Create content using **Typed fields & composition**, select the content type and enter facts in named controls. Publish through the ordinary editor. With JavaScript disabled, the server still offers installed content types and its explicit **Fields (JSON)** fallback; it does not yet provide the enhanced named-field editor. An invalid typed value fails before content mutation. Changing working values leaves the current publication intact.

Readers see **Published facts** assembled from the same projection used in the page's JSON-LD. Theme authors do not maintain another copy of these facts. A private preview includes dated guidance warnings and inspectable graph JSON, but emits no public JSON-LD script. Previewed root facts use working values; related entities always use currently public publications.

## Public people and multiple locations

An Article helper binds its visible published title, original publication time, canonical URL and language. A search-snippet title is separate from the factual headline. Publication does not invent a modification date. Create a separate public Person or Organization model and relationship field to identify authors/publisher explicitly; administrative account names/emails never become author facts.

For locations, each native record can have its own business name, actual address, coordinates, telephone and opening-hour rows. Choose the most specific accurate business subtype, such as Restaurant, rather than misrepresenting every location as the same generic business. Opening times accept `HH:MM` or `HH:MM:SS`; weekday choices bind to known enum values. An Organization model can bind a `relationships` field targeting the location model to its `location` property. Publish each location and the organization; the organization page emits the distinct linked locations and their admitted address/hour entities. This is an ordinary shared content relationship, not a separate location database.

Use breadcrumb rows in actual order, with distinct positive positions, truthful names and actual target page URLs. Internal entity fragment identities are not substitutes for target page URLs. Review selected provider warnings in a preview.

## Binding grammar

The optional model key `structured` contains `vocabulary: "30.1"`, `primary`, an `entities` map and optional `allow_pending: false`. Each named entity has one to four known `types`, optional `scope` and `properties`. Property values are one of the following tagged declarations:

| Binding | Meaning |
| --- | --- |
| `field`, `path`, `datatype` | A declared native scalar field, interpreted as Text, URL, Date, DateTime, Time, Number, Integer or Boolean. |
| `enumeration`, `path`, `choices` | Map every declared native choice to a known admitted vocabulary enum value. |
| `constant_enumeration`, `value` | An explicit known enum value; never arbitrary JSON. |
| `entity`, `entity` | Reference another named entity in the same record. |
| `relationship`, `path`, `entity` | Reference a named entity of currently public related records. |
| `image`, `path` | An admitted local image with actual dimensions; private/restricted media and non-images are omitted. |
| `publication`, `attribute` | Factual published `title`, `url`, `published_at` or `language`. |

Paths have one to eight safe native field names. A scope selects an object/group or a bounded repeater; property paths are relative to that scope. Named references may be cyclic: visited identities prevent recursive growth. Absent fields omit their property; no placeholder business facts are fabricated. See `tests/structured.rs` and the connected structured acceptance journey for readable complete examples.

The native admission policy checks known terms, type ancestry, expected associations and typed ranges. Schema.org's documented pragmatic Text/URL fallbacks are accepted for explicit field bindings; publication convenience bindings remain semantically typed. This is bounded application policy, not a full open-world ontology/JSON-LD processor. Remote contexts, arbitrary scripts and raw graph content are not accepted. Editorial definitions cannot claim native catalog price, currency, availability, inventory, offers or price-specification authority; authoritative commerce enrichment remains D16.

## Privacy, limits and guidance

Related drafts, unpublished or currently restricted records are omitted even when historical publication fields retain their IDs. Admitted media uses current resource policy. Protected root responses use existing private/no-store/noindex policy and omit public JSON-LD. Recovery validates both working and publication field semantics before restore.

Initial native ceilings are 16 entities and 128 bound properties per model, 2,048 bound properties across the registry, 64 expanded entity identities and 64 KiB serialized graph within the existing 2 MiB page context. Existing content, repeater, relationship-depth and media budgets also apply. Output that exceeds these limits fails rather than silently truncating facts. Vocabulary search returns at most 40 entries per page and supports a bounded ASCII prefix/cursor.

Schema.org vocabulary admission and Google search-feature guidance are different checks. Selected Article checks are recommendations rather than universally required properties. The LocalBusiness profile requires a factual name and physical PostalAddress. Breadcrumb guidance expects at least two ListItems. Google FAQ rich results retired in May 2026 even though FAQPage remains a vocabulary type. Warnings cannot establish indexing, rich-result eligibility, relevance or ranking; profiles are selected and dated, not exhaustive.

Offline vocabulary source, hashes, normalization and CC BY-SA 3.0 attribution are recorded in `docs/research/d07-vocabulary-provenance.json` and THIRD_PARTY_NOTICES.md. Developers run `python3 scripts/schema_vocabulary.py --check` to verify reproducibility. D07-01/D07-02 records in `docs/evidence/feature-guidance.json` define authoritative sources, checked dates, refresh triggers and the release freshness gate. A vocabulary or provider-rule update requires reviewed changes to provenance, admission fixtures, helpers, warnings and guidance records; installed sites retain their bundled offline rules until explicitly upgraded.

## Upgrade and recovery

Stop application and workers. Follow the maintenance preview/apply workflow in distributed-operations.md with an independent encrypted recovery point. Native-20 sites are refused by ordinary native-21 startup; maintenance migrates them explicitly. Portable-v16 archives are refused by ordinary restore: convert offline with `migrate-recovery-v16`, then inspect/restore the resulting v17 archive. Preserve the original archive and original executable for fresh old-runtime rollback. Newly structured model keys disguised under older archive markers are rejected. Old grammar conversion is separate from current runtime behavior; no permanent compatibility parser is retained.
