# Independent local AI draft worker

An optional Python 3 standard-library process communicating with wpalt API v1 and an independently operated local [Ollama generation endpoint](https://docs.ollama.com/api/generate). The Rust application does not load or launch this process and runs normally when it or the model is absent. Choose/install a local model in your own Ollama instance; model resources are separate from CMS footprint measurements. No cloud account or hosted inference is needed. The worker only admits literal loopback AI endpoints and refuses redirects/proxies.

1. Create a content-read credential in **Operations → Integrations** or through `wpalt integration create`. Its grant includes private and draft content, so explicitly trust the process. Protect the downloaded file with private permissions (`chmod 600 worker-token.txt` on Unix). A CLI-created file is already private.
2. Run a proposal against one chosen native content UUID and an installed local model:

```
python3 local_ai_worker.py --site http://127.0.0.1:8080 --token-file worker-token.txt \
  suggest --source CONTENT_UUID --slug suggested-story \
  --model YOUR_INSTALLED_MODEL --output new-private-proposal.json
```

The worker reads only that selected source body into the local model prompt. It writes a new private proposal with source identity/version/body hash, model, native draft input and site binding. Console output contains the review plan, without source/generated text or credentials. Keep the proposal private and read its actual suggested text; a successful protocol call does not establish factual correctness.

3. Supply a credential additionally granted content:draft, review the exact proposal, then apply:

```
python3 local_ai_worker.py --site http://127.0.0.1:8080 --token-file draft-token.txt \
  apply new-private-proposal.json --execute REVIEWED_PLAN
```

It checks the current source version/body before requesting a new unscheduled draft. This preflight is not an atomic source-and-draft transaction; later source edits never get overwritten. The server rechecks the integration credential at its mutation boundary, enforces native validation and denies publication. The unique output slug prevents an uncertain retry from duplicating drafts; inspect native state after any timeout before retrying. Edit/publish through the native owner/editor workflow. Revoke the credential in Operations when done. Model output has no tool invocation, arbitrary code execution or access-grant path.

Boundaries: private regular input files, no final symlink following on Unix, two MiB response cap, 30-second transport timeout, bounded source/generated body and model name, 2,048 requested output tokens and explicit review-bound new-file creation. Set model/OS process limits independently. Sensitive article text is deliberately granted to the local model; its own logs/storage are the owner's responsibility. Non-Unix filesystem permissions differ. The local contract fixture proves process/API/privacy/failure/retry boundaries, **not actual model inference quality or performance**; actual installed-model reference evidence remains pending.

Verification: `python3 scripts/integration_acceptance.py --binary PATH` from the repository exercises this independently operated process against the actual Rust executable and a synthetic loopback generation server, including read-only/write-scope distinction, stale plans/source, denied cloud endpoints, draft-only creation and duplicate retries. The standalone distribution carries this example as `local_ai_worker.py` with this guide; Python and Ollama remain optional owner-operated dependencies.


## Independent webhook worker

Unix/Python 3 `webhook_worker.py` is a second optional process. Put both worker scripts in the same directory (as supplied in the distribution). Create a private checkpoint directory (`mkdir -m 700 delivery-state`), a private content-read token file, and a private signing-key file with at least 32 random bytes. Run through your OS scheduler:

```
python3 webhook_worker.py --site http://127.0.0.1:8080 --token-file read-token.txt \
  --destination https://receiver.example/events --signing-key-file signing-key.bin \
  --checkpoint delivery-state/receiver.json
```

Each invocation handles at most 25 retained content metadata events. Invoke again when `has_more` is true. Receiver requirements: verify HMAC-SHA256 over the exact raw JSON bytes with constant-time comparison, deduplicate `X-Wpalt-Event-ID` durably, and return 2xx only after accepting the event. Private source bodies and CMS credentials are never forwarded. Delivery is at least once: crashes can replay receiver-accepted events. A failed transport or 409 gap/reset pauses progress. Perform initial content synchronization and explicit reconciliation after lost history or portable recovery; the initial empty cursor only replays currently retained records. Only a fresh checkpoint can accept `--start-at RECONCILED_CURSOR`. Endpoint changes require separately reconciled state. Do not delete checkpoints to hide a failure.

The CMS emits content-save/scheduled-publication events transactionally and bounds its journal with `[integration_events] retained_events`. Business/commerce/member changes are outside this initial event contract. The full transport and physical/portable recovery boundaries are in the distribution's `migration-and-extensions.md`.
