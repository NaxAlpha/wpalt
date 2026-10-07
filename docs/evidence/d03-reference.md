# D03 primary reference evidence

Checked 2026-10-07 (Tokyo). Reference semantics guide independently implemented local workflows; neither documentation nor static inspection establishes full plugin parity.

## Free Polylang source

Downloaded [Polylang 3.8.10](https://downloads.wordpress.org/plugin/polylang.3.8.10.zip) from WordPress.org. Archive: 720,901 bytes; SHA-256 `3bd880d9006d7dc284978240babacd87f86e6fa2a16bcc410857b52b7ac23154`. The package header/readme identifies version 3.8.10 and GPLv3 or later. Ignored local copy: `work/reference/polylang-3.8.10`; not executed, redistributed or copied into application code. PHP/container execution is not available in this development environment; this is static source inspection.

Inspected `src/modules/sync/sync-post-metas.php`, `sync-metas.php` and `settings-sync.php`. The free implementation selects metadata for copy/synchronization, distinguishes protected WordPress meta keys, and has separately selected special metadata such as featured-image/page-template values. A protected WordPress metadata key is not the same concept as a member-access-protected article. wpalt therefore checks native resource policies explicitly instead of treating a metadata naming convention as an access rule. Taxonomy, page hierarchy and translated media mappings remain richer source behavior; the current wpalt operation deliberately copies only explicitly selected declared native fields and does not imply parity with those mappings.

[Vendor synchronization guidance](https://polylang.pro/documentation/support/guides/synchronize-metadatas-between-translations/) and [duplication guidance](https://polylang.pro/documentation/support/guides/duplicating-content-across-post-translations/) distinguish language versions and configurable shared values. Paid duplication behavior is documentation-only; no premium package has been installed or verified.

## Language declarations

[W3C language declarations](https://www.w3.org/International/questions/qa-html-language-declarations.html) identify the actual content language on the root and language changes on relevant descendants. Direction is a separate property. The language workspace sets each saved comparison's language/direction from its configured content language; proposed account interface preferences must not alter those values. Mixed-direction identifiers require isolation, and untranslated interface sections must retain an honest English declaration.

## Initial catalog scope

The bundled catalog covers selected navigation, interface preference and language-workspace messages in English/French/Japanese/Arabic. It is an authored initial translation set, not professionally certified localization. Other screens retain English until their connected states are translated and verified. Stable IDs, escaped parameter text, bounded immutable local loading, English fallback, localized integer formatting and count grammar are code boundaries to verify; they do not establish translation quality or universal locale coverage.

## Model transport and integer language grammar

Checked 2026-10-07: [Ollama generation API](https://docs.ollama.com/api/generate) describes non-streamed responses, schema-constrained `format`, `think`, `keep_alive`, completion reasons and nanosecond/token metrics. The worker requests a bounded JSON text result and rejects output-budget exhaustion; schema-valid text remains untrusted translation requiring human review. The separately installed model digest is bound to resumable work. No hosted inference is inferred from literal-loopback availability.

[Unicode CLDR 48 cardinal language rules](https://www.unicode.org/cldr/charts/48/supplemental/language_plural_rules.html) ground the scoped integer-count English/French/Japanese/Arabic catalog behavior. Decimal/ordinal/currency/date formatting is outside this initial integer message contract. Authored wording still needs human language review; matching a category is not professionally certified localization.
