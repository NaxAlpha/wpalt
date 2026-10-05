# M9 capability reconciliation for owner review

This is an inventory of current evidence and remaining scope, not a declaration of universal WordPress/plugin parity. The product permits modern native alternatives; it does not assume unchanged PHP plugins/themes can execute. Prior milestone defined scopes remain bounded by their recorded limits. Multiple local processes replace the physical multi-host requirement for this delivery under the owner’s explicit direction.

## Decisions still requiring alignment

- **F024:** native composable themes and explicitly lossy Elementor heading/text imports work; exact widget/style/template/display-condition compatibility does not. Options are retain a separate compatibility project or accept native replacement plus explicit migration losses. No choice has been inferred from silence.
- **F129:** scoped external hooks, durable content events, owner-hosted worker examples and hash-pinned consent scripts work. Arbitrary runtime server snippets are not implemented. Broader language/runtime/sandbox behavior needs an explicit supported extension contract.
- **F130:** independently running read-only fleet observation works, including origin failure and credential revocation. Bulk update/repair orchestration does not. Read-only delivery is a defined slice, not full fleet-management parity.
- License and public support baseline require owner decisions. A public repository and nightly artifact do not alone establish production support.

## Newly verified local capabilities

F009 provides native exact-plan translation draft duplication and selected field synchronization; F011 offers reviewed short-passage local-model translation proposals. F031/F034 provide a bounded canonical-publication link graph and advisory text/keyword facts. These are owner CLI workflows, not new administration screens. Translation quality remains model-dependent and human reviewed; graph suggestions omit theme/navigation/dynamic links.

F124 validates one static module ownership/dependency registry. F125 transfers current-version private configuration and the native portable settings/schema graph. F126/F127 provide bounded cycle/stage history, distinct worker processes, finite graceful drain and explicit interrupted-work retry. F128 provides stopped-site recovery/upgrade, reviewed safe pause/rebind and fresh-target rollback. These capabilities are traceable below; they do not imply dynamic module unloading, mixed-version rolling writes, automatic failover or exactly-once external effects.

## Complete catalog inventory

Each identifier remains linked to the canonical matrix and original local/cloud classification. Evidence status means the recorded defined slice; consult its milestone contract and coverage fields before treating the entire plugin family as implemented.

| ID | Cluster | Capability | Recorded status |
|---|---|---|---|

| F001 | content | Custom types and taxonomies | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F002 | content | Typed custom fields and relationships | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F003 | content | Repeaters, galleries and clone groups | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F004 | content | Flexible content / reusable block schemas | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F005 | content | Global options / settings pages | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F006 | content | Content revisions and rollback | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F007 | content | Drafts, preview and editorial approval | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F008 | content | Manual language variants and strings | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F009 | content | Translation duplication / synchronization | in-progress-M9-defined-local-scope |

| F010 | content | Multilingual slugs and metadata | verified-local-defined-multilingual-publication-scope |

| F011 | content | Automatic translation with local models | in-progress-M9-defined-local-scope |

| F012 | content | Vendor automatic translation / human network | optional-connector-or-external-boundary |

| F013 | design | Responsive blocks and layout primitives | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F014 | design | Theme-wide templates and dynamic field bindings | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F015 | design | Reusable patterns and design tokens | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F016 | design | Header, footer and navigation builder | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F017 | design | Tabs, accordions, galleries and carousels | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F018 | design | Conditional / protected presentation | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F019 | design | Custom CSS and approved asset insertion | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F020 | design | Template import/export and cross-site reuse | verified-local-defined-M2-scope; CI-status-on-PR-3 |

| F021 | design | Collaborative notes and assignments | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F022 | design | Cloud template marketplace/library | optional-connector-or-external-boundary |

| F023 | design | AI layout/content generation locally | implemented-defined-M8-scope: actual local content and bounded theme-layout proposals verified; no general generation-quality guarantee |

| F024 | design | Exact Elementor template/widget compatibility | in-progress: selected 0.4 heading/text projection with exact loss reports; full styles/widgets/conditions incomplete |

| F025 | seo | SEO titles, metadata and social previews | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F026 | seo | Canonical, robots and indexing rules | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F027 | seo | XML, image, video and news sitemaps | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F028 | seo | Schema graph and custom schema builder | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F029 | seo | Local business / multiple location schema | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F030 | seo | Redirects, regex rules and 404 reports | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F031 | seo | Internal-link graph / orphan detection | in-progress-M9-defined-local-scope |

| F032 | seo | Internal broken-link checks | implemented-defined-M3-scope; current-head verification recorded on milestone PR |

| F033 | seo | External broken-link checks | optional-connector-or-external-boundary |

| F034 | seo | Readability and keyword heuristics | in-progress-M9-defined-local-scope |

| F035 | seo | Search Console / search index data | optional-connector-or-external-boundary |

| F036 | seo | SERP ranking / competitive keyword datasets | optional-connector-or-external-boundary |

| F037 | forms | Form designer and reusable field schema | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F038 | forms | Conditional fields / notification routing | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F039 | forms | Multi-step forms and repeatable inputs | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F040 | forms | Entry storage, search, CSV export and permissions | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F041 | forms | File uploads and protected attachments | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F042 | forms | Calculations and pricing logic | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F043 | forms | Signatures and signed-document records | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F044 | forms | Surveys, polls and scoring | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F045 | forms | Save/resume, partial entries and abandonment | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F046 | forms | Offline capture and later synchronization | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F047 | forms | User registration and post creation | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F048 | forms | Webhook and CRM/marketing handoff | optional-connector-or-external-boundary |

| F049 | forms | Payment fields / gateway checkout | optional-connector-or-external-boundary |

| F050 | forms | External CAPTCHA service | optional-connector-or-external-boundary |

| F051 | identity | Local users, roles and granular permissions | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F052 | identity | Content and download access rules | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F053 | identity | Membership state and entitlement administration | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F054 | identity | Content drip and timed unlocks | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F055 | identity | Course lessons and learner progress | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F056 | identity | Quizzes, assignments, certificates and gradebook | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F057 | identity | Corporate subaccounts and gifting | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F058 | identity | Community profiles and discussion groups | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F059 | identity | Affiliate referral and commission records | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F060 | identity | Enterprise identity / social-login provider | verified-local-defined-M5-scope; exact-CI-and-artifact-status-on-PR-10 |

| F061 | commerce | Catalog, variants and digital products | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F062 | commerce | Cart, checkout state and orders | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F063 | commerce | Inventory and reservation locks | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F064 | commerce | Coupons, discounts and member pricing | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F065 | commerce | Booking calendars, staff/resources and group capacity | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F066 | commerce | Local appointment confirmations and reminder scheduling | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F067 | commerce | Subscription schedules, proration and dunning state | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F068 | commerce | Refund/admin transaction workflow | verified-local-defined-M6-scope; final-CI-status-on-PR-12 |

| F069 | commerce | Card settlement and recurring collection | optional-Stripe-HTTPS-adapter-verified; provider-account-sandbox-not-certified |

| F070 | commerce | Shipping rates/labels and calendar sync | optional-connector-or-external-boundary |

| F071 | commerce | Current tax/compliance datasets | optional-connector-or-external-boundary |

| F072 | media | Page caching, invalidation and preload | M7-verified-defined-scope |

| F073 | media | Browser-cache headers and compression settings | M7-verified-defined-scope |

| F074 | media | Object-cache integration | M7-verified-defined-scope |

| F075 | media | Asset minification and loading controls | M7-verified-defined-scope |

| F076 | media | Lazy loading, dimensions and preload hints | M7-verified-defined-scope |

| F077 | media | Resize, compress and WebP/AVIF derivatives | M7-verified-defined-scope |

| F078 | media | Native image binary / imgproxy worker | M7-verified-defined-scope |

| F079 | media | Unused/critical CSS generation | M7-verified-defined-scope |

| F080 | media | Unused-image and database cleanup | M7-verified-defined-scope |

| F081 | media | Geolocation / role-aware cache variants | M7-verified-defined-scope |

| F082 | media | Video transcoding on own server | M7-verified-defined-scope |

| F083 | media | Global image/video CDN and edge WAF | optional-connector-or-external-boundary |

| F084 | analytics | First-party pageviews, events and conversions | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F085 | analytics | Forms/store funnels and custom dimensions | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F086 | analytics | Local dashboards and reports | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F087 | analytics | Heatmaps and masked session replay | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F088 | analytics | A/B experiments and variant allocation | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F089 | analytics | Exit-intent, device and referrer popups | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F090 | analytics | Scheduled/cookie/visitor-based targeting | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F091 | analytics | On-site coupon wheels / offer campaigns | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F092 | analytics | Google/ad-platform reporting datasets | optional-connector-or-external-boundary |

| F093 | analytics | Cross-site audience / advertiser identity graph | optional-connector-or-external-boundary |

| F094 | communications | SMTP settings and connection routing | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F095 | communications | Mail logs, resend, failure alerts and fallback | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F096 | communications | Transactional templates and notification rules | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F097 | communications | Mailing lists, consent records and segmentation | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F098 | communications | Newsletter design, queue and automation | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F099 | communications | Self-operated outbound mail server | verified-local-defined-M4-scope; required-current-head-CI-and-review-on-PR-8 |

| F100 | communications | Chosen SMTP/transactional mail provider | optional-connector-or-external-boundary |

| F101 | communications | Gmail/Microsoft or remote CRM connectors | optional-connector-or-external-boundary |

| F102 | communications | Global inbox deliverability / shared reputation service | optional-connector-or-external-boundary |

| F103 | security | TOTP/passkeys and local login protections | M7-verified-defined-scope |

| F104 | security | Security headers and hardening | M7-verified-defined-scope |

| F105 | security | Local request rule engine and rate limits | M7-verified-defined-scope |

| F106 | security | File-integrity / known-pattern malware scans | M7-verified-defined-scope |

| F107 | security | Audit logs and privileged action history | M7-verified-defined-scope |

| F108 | security | Honeypots, proof-of-work and basic spam heuristics | M7-verified-defined-scope |

| F109 | security | Cookie banner and consent/script enforcement | M7-verified-defined-scope |

| F110 | security | Consent records and data-request workflows | M7-verified-defined-scope |

| F111 | security | Cookie scan on own pages | M7-verified-defined-scope |

| F112 | security | Fresh vulnerability/reputation/signature datasets | optional-connector-or-external-boundary |

| F113 | security | Akismet-style shared spam classification | optional-connector-or-external-boundary |

| F114 | security | DDoS absorption and managed incident response | optional-connector-or-external-boundary |

| F115 | resilience | Consistent files + database snapshot | M7-verified-defined-scope |

| F116 | resilience | Schedules, retention and pre-update backups | M7-verified-defined-scope |

| F117 | resilience | Encrypted backup packages and recovery keys | M7-verified-defined-scope |

| F118 | resilience | Incremental/deduplicated backup worker | M7-verified-defined-scope |

| F119 | resilience | Selective restore, clone and URL-aware migration | M7-verified-defined-scope |

| F120 | resilience | Portable restore utility independent of CMS | M7-verified-defined-scope |

| F121 | resilience | Off-site copy to owner SFTP/NAS/secondary host | optional-connector-or-external-boundary |

| F122 | resilience | Point-in-time recovery and continuous replication | M7-verified-defined-scope |

| F123 | resilience | Vendor vault / off-site disaster recovery service | optional-connector-or-external-boundary |

| F124 | operations | One module registry and ownership resolver | in-progress-M9 |

| F125 | operations | Unified settings/schema export and import | in-progress-M9 |

| F126 | operations | Background jobs, retries and health history | in-progress-M9 |

| F127 | operations | Reliable system-scheduled worker | in-progress-M9 |

| F128 | operations | Local safe-mode and rollback controls | in-progress-M9 |

| F129 | operations | Authorized snippets and developer hooks | in-progress-M9 |

| F130 | operations | Owner-hosted fleet controller | in-progress-M9-defined-local-scope |

| F131 | operations | Uptime alerts while origin is down | optional-connector-or-external-boundary |

| F132 | operations | Vendor license renewal / cloud quotas | optional-connector-or-external-boundary |


The machine-readable [matrix](../feature-parity.json) retains implementation/coverage/evidence for every entry. Optional external boundaries remain optional; they were not converted into local features or silently discarded. Current delivery verification is separate from owner acceptance of the scope decisions above.

## Owner decision — 2026-10-06

The owner explicitly directed all remaining missing features into later work and authorized making PR #16 ready and merging. The decision questions above are preserved historically and resolved for current delivery by this deferral. [The actionable cross-milestone backlog](../deferred-features.md) and [decision 0014](../decisions/0014-deferred-parity-release.md) retain the original broader goal without claiming full parity. License/support remain deferred adoption choices, not a merge hold.
