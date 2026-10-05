# Independent Field journal theme

This native package is independently authored for wpalt, without PHP, hosted assets or a vendor account. It composes navigation, cardless publication listings, bounded derived excerpts and a readable article template. Colors use a warm neutral background, dark text and green links; typography uses locally available serif fonts.

Stop the configured site and run:

```
wpalt --config SITE.toml theme validate field-journal.json
wpalt --config SITE.toml theme import field-journal field-journal.json
```

Import saves an unpublished draft. Inspect in Design studio, then explicitly publish and activate. The package needs the standard native content context and no external file/network loader. It can also serve as the base for the separately operated local AI layout worker. The meaningful public browser journey is `WPALT_THEME_ONLY=1 node scripts/browser_acceptance.cjs` with a configured Playwright/Chrome runtime. That journey also tests a three-column layout collapsing to one column on mobile; the base itself uses simple stacked listings.
