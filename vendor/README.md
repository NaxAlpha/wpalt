# Temporary encoder maintenance patch

`rav1e` 0.8.1 is the complete checksummed crates.io source (SHA-256 `43b6dd56e85d9483277cde964fd1bdb0428de4fec5ebba7540995639a21cb32b`, original VCS `38806721f718e7ae6919e72385bc305bf824ef81`). Its registry release still uses unmaintained `paste`; we apply the upstream [pastey migration](https://github.com/xiph/rav1e/commit/c247d53ae43dd1312dbd90117c45e4c0ee6b06ce) and two explicit elided-lifetime annotations required for warning-free newer compilers. No encoder algorithm or assembly changes.

`python3 scripts/verify_encoder_vendor.py` checks every file against the original registry archive plus the exact transformations. Clean-build CI runs this before compilation. Keep LICENSE/PATENTS intact. This adds development-source bytes, not deployed source files or a runtime service. No advisory suppression or shim named `paste` is used.

Removal condition: replace this path patch with a reviewed registry encoder release that has the maintained macro dependency, passes Rust 1.88, selected-graph audit, real AVIF/browser decode and footprint checks. Review monthly and on upstream changes through the M7-NATIVE-AVIF guidance record.
