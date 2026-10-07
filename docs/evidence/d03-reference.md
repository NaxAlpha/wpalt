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
