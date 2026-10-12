# Offline library and requested response recovery

D12 is in development; verification/delivery is pending. Native25/portable20 introduces publication-bound recovery policy and private intent tables. See [contract](d12-contract.md).

## Deliberately save on a private device

Use a published article’s Save for offline reading link, or a form’s Save an offline form draft link, then choose Save on this device in `/offline/`. Nothing is saved by browsing. The dedicated shell and worker serve only the `/offline/` library. Snapshots contain public article text or eligible scalar form grammar; they exclude working copies, member content, remote media, theme HTML, attachment capabilities, signatures, acknowledgments and account/subscription forms. Interactive content requires an online visit.

The shell is bound to exact shared-design/form/script hashes and validates those bytes during installation. Compatible interface updates preserve public snapshots and original expiry; unsupported cache grammars are cleared. The worker admits one storage operation at a time.

Limits: 512 KiB per public snapshot, 30 publications, 2 MiB total snapshots, seven days from explicit save. Delete/Clear removes device copies and applicable device form drafts. HTTPS or localhost and Service Worker/Cache support are required; denied or evicted storage yields a readable failure while the live site remains usable. Offline copies cannot discover later server revocation until reconnection. A live denial deletes its saved snapshot; a changed publication warns rather than replacing saved work automatically. Re-saving is a deliberate refresh.

Offline form editing produces a device draft, never a submission or automatic synchronization. Reconnect and review on the live form to send. If a publication changes, old answers remain explicitly visible and unmapped; abandon the original draft before starting again. An unresolved interrupted response keeps its exact payload/identifier so a successfully accepted original response can be recognized after republishing. Original retry does not run newer required-field validation. Do not infer consent from a saved or abandoned draft.

## Request specific response recovery

The editor’s Response recovery policy workspace selects a specific purpose and 1..16 unconditional scalar fields for the current form publication. Attachments, signatures, acknowledgments, calculations, conditional fields and account/subscription actions are ineligible. Publishing a new policy or form edition fences earlier delivery permissions; enabling recovery never exposes unrelated private drafts.

The visitor explicitly requests selected-value owner review and one reminder to a supplied email address. A separate confirmation message contains no answers. Opening that link only loads private controls; choose Confirm address and recovery permission to confirm. Only current-policy confirmed partial values appear in the editor workspace. One reminder is scheduled one day after confirmation. No audience membership or marketing subscription is created.

The private fragment capability permits restoring selected values for review, downloading an export, and withdrawing/deleting the partial record. It is removed from the address bar before requests; server storage holds hashes. Complete matching submissions atomically remove recovery records and cancel queued reminders. Publication/policy changes, deletion and expiry also fail worker preflight, including after lease. Delivery already completed or made uncertain cannot be undone. Mail recipient/control-link retention and independent backups follow their separate operational retention policies. Shared-device visitors must delete local copies themselves.

Recovery records expire seven days from creation, share configured `business_limits.drafts`/`draft_bytes` admission with private server drafts, and have a 1,000-record per-form limit and 16 KiB selected-value budget. Worker cleanup removes at most 50 expired records per maintenance pass. Mail uses existing bounded dedupe/admission, local private outbox or configured SMTP; enabling SMTP requires genuine owner credentials. With mail disabled, recovery requests refuse explicitly.

Account data export additionally includes confirmed-email recovery records belonging to the account’s current address, without token hashes or unrelated/unconfirmed records. Visitor capability export/delete is the direct anonymous-subject route. Account erasure is explicitly partial; remaining form/recovery records require separately verified owner/visitor handling rather than an invented identity match.

## Upgrade and independent recovery

Stop all app/worker nodes, retain the original D11 executable/configuration and an independent backup. Current serve refuses native24. Use the reviewed `upgrade` preview then exact plan execution with a new private recovery output/key. Native25 adds empty consent tables without inventing visitor permission or rewriting historical forms/answers. Retained original D11 backups are portable19: convert offline with `migrate-recovery-v19 INPUT NEW_OUTPUT [--key-file KEY]`, then restore portable20 into a fresh target. Ordinary restore accepts only the current graph. Older documented converters chain into portable20. Test original-source rollback and fresh recovery before adopting meaningful data. URL-rewritten held clones remove recovery policies/intents and cancel queued recovery mail; a different site must obtain fresh permission rather than inherit visitor consent.

Primary documentation reviewed 2026-10-13: [Service Workers](https://developer.mozilla.org/en-US/docs/Web/API/Service_Worker_API), [Cache](https://developer.mozilla.org/en-US/docs/Web/API/Cache), [WPForms offline forms](https://wpforms.com/docs/how-to-install-and-set-up-the-offline-forms-addon/) and [form abandonment](https://wpforms.com/docs/how-to-install-and-use-form-abandonment-with-wpforms/). Vendor paid-add-on documentation is not an executed paid-runtime parity claim. Our finite budgets, explicit consent, text/scalar projections and seven-day policy are deliberate product rules.

Snapshot fetches use `credentials: "omit"`; browser credential omission also prevents response-cookie processing. JavaScript cannot inspect the forbidden `Set-Cookie` response header. The native anonymous snapshot route does not set cookies. See [Request credentials](https://developer.mozilla.org/en-US/docs/Web/API/Request/credentials) and [forbidden response headers](https://developer.mozilla.org/en-US/docs/Glossary/Forbidden_response_header_name), reviewed 2026-10-13.

Offline initialization has a 20-second UI deadline; each release-asset fetch has a 10-second deadline. A failed integrity/installation check reports a readable reconnect/live-site action and never activates the changed shell. Navigation respects reduced-motion preferences.
