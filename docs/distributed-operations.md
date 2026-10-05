# Distributed operations — M9 work in progress

Do not run multiple writers against a site based on this preparation document. The current application still has process-local authority, cache and challenge boundaries; distributed startup is not certified. Supported M8 runtime remains one application process per site/data directory.

## Module admission and ownership

`wpalt --config site.toml modules` prints the current `wpalt-module-inventory-v1` report. CLI/environment overrides use the same effective configuration precedence as `config` and `serve`; configuration validation runs first. This operation does not open a database, create a site directory or require an initialized site. It contains no connection strings, account details or credentials.

The registry describes publishing, business, engagement, membership, commerce, operations and extensions. Engagement admission includes both its own switch and the business parent switch. Ownership names identify domain responsibility; they are not permission grants, a dynamic code loader, proof that disabled code is removed from the executable, or cluster readiness. Existing APIs retain their native authority/feature checks.

Configuration transfer, schema export/import, coordinated worker execution and fleet controls are later working steps within the active M9 contract. See [coordination inventory](evidence/m9-coordination-inventory.md) and [M9 contract](m9-contract.md) for obligations and acceptance gates.
