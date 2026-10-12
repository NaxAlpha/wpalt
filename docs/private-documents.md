# Private attachments and response documents

D11 adds optional local scanning and exact-byte server attestations to frozen form responses. SQLite and PostgreSQL use the same owner workflow. Both features default off; no cloud account, subscription or runtime Node is needed. Native 24 / portable 19 stay unchanged: accepted publications/answers/attachments already contain the source data. No downloaded artifact or key is inserted into site backups.

## Enable owner-hosted scanning

Install and isolate a maintained ClamAV daemon on a Unix host. Keep its official signature intelligence current with your chosen operational process (typically FreshClam); the development fixture uses one harmless custom signature and proves neither current intelligence nor complete malware detection. Restrict the Unix socket to the application/daemon users, configure `StreamMaxLength` to at least 2 MiB, and size daemon threads/queue for the sum of all application nodes' admission. An unauthenticated remote TCP scanner is not supported.

```toml
[attachment_scanning]
enabled = true
socket = "/run/clamav/clamd.ctl"
deadline_seconds = 5
concurrency = 2
```

These are per-process work slots and a complete stream-operation deadline. Uploads scan original bytes before image decoding; normalized images pass a second scan before any file/quota/capability is committed. Private downloads re-scan integrity-checked bytes under current policy. Detected, missing, slow, malformed or over-budget engines refuse the operation. Refused uploads create no capability, file or quota. Previously accepted files remain stored when a download is refused, so an operator can investigate; they are not silently deleted or relabeled safe. Disabled scanning is explicitly shown in collection/review. Existing file types, 2 MiB bound, private permissions, expiration/single-use capabilities and authenticated downloads still apply.

## Configure attestations

Create a new key in an owner-controlled private directory; the command refuses to overwrite files:

```sh
./wpalt document-key /srv/wpalt-keys/response.pk8
```

It prints the public-key SHA-256 fingerprint only. Preserve that fingerprint through an independent trusted channel. Keep the private key mode 0600, regular, single-link and inaccessible to other users; symlink keys are refused. Back it up separately using an appropriate encrypted/key-management process. Never put it in Git or a public document. Enable and restart:

```toml
[documents]
enabled = true
key_file = "/srv/wpalt-keys/response.pk8"
pdf_enabled = false
chromium = ""
```

CLI/config loading uses the same typed configuration; `./wpalt --config site.toml config` gives the ordinary redacted inspection. Enabled missing/invalid/insecure keys refuse startup. To recover source records without the lost key, explicitly disable signing; that cannot regenerate a signature under the original key. For rotation, preserve original public-key trust, stop all local nodes and worker, replace the private key deliberately, then use the reviewed stopped `local-resume` plan in distributed-operations.md before restarting. Key identity is bound to node configuration; a new node cannot silently join with different bytes at the same path. A running process retains its loaded key until restart. External physical-host distribution/key custody remains an operator responsibility.

## Owner workflow and independent verification

Open Forms → response list → the original response. Select printable HTML or enabled PDF and prepare its attested document. Modern browsers on a secure context check Ed25519, the site's displayed key, source response/form, artifact format/length and SHA-256 before exposing document and bundle downloads. Keep both. Errors preserve the response and format choice; tampered bytes/key/signature are refused before extraction. If WebCrypto is unavailable, the native form downloads the bundle. Without JavaScript the same native download works.

The site-displayed fingerprint check protects extraction consistency; independent external verification still needs a fingerprint obtained through a trusted channel. To check a retained bundle and extract a new private artifact:

```sh
./wpalt document-verify response-document.json \
  --trusted-key-sha256 YOUR_INDEPENDENTLY_TRUSTED_64_HEX_FINGERPRINT \
  --output response.html
```

Use `response.pdf` for a PDF bundle. Existing output files are never overwritten. Verification prints validity, media type, bytes, digest, public fingerprint and authority without source answers. It continues working after rotation with original public trust. A key supplied beside its signature is not independently trusted merely because they match.

The signature covers `wpalt-response-document-attestation-v1` plus a NUL byte plus the exact UTF-8 manifest bytes. Ed25519 follows RFC 8032. The manifest binds exact artifact bytes/digest/length, frozen definition digest/version, response ID/time, canonical visible accepted values, original acknowledgment and private attachment digests. It signs serialized exact bytes, not a claim of RFC 8785 JSON canonicalization. The PDF signature is detached in the bundle; it is not an embedded PAdES signature. Original terms survive republication; current follow-up notes and later terms are excluded. A typed name remains a declaration: no verified respondent identity, private-key possession, legal certification or qualified signature is implied.

## Optional PDF renderer

Set `pdf_enabled = true` and an absolute maintained Chromium or `chrome-headless-shell` path. Use a restricted application/worker user and OS policy to isolate network, memory, CPU and disk/process capacity. The executable is owner-controlled; never substitute an untrusted command. No shell or user-selected flags execute. The application passes only escaped locally generated HTML with a restrictive CSP, a fresh private source/profile/output directory, fixed flags, cleared inherited environment, a 20-second deadline and one document work slot per app process. Cancellation/deadline kills the owned Unix process group and removes temporary data; output is bounded to 8 MiB and manifests to 256 KiB. Host resource/network limits are additional requirements: headless flags/timeouts are not OS containment. Account for all nodes and scanner/browser workers separately from application RSS.

Actual installed full Chrome 155 on the development Mac wrote a PDF then returned exit 2 after a teardown watchdog failure. wpalt correctly refused it. The separately installed Chromium 148 headless shell passed with the same secret-free worker settings; CI records its actual renderer version. A written file cannot turn a failed worker into success. Fonts and pagination depend on the host. Inspect representative documents after renderer/font upgrades. Preserve downloaded PDFs: later rendering metadata is not promised byte-identical. The default HTML remains self-contained.

## Verification and maintenance

See d11-contract.md and evidence/d11-verification.md for current delivery status. Readable native clusters check independent OpenSSL verification, tampering/wrong trust, original statements/fresh recovery/current permission on both engines, worker refusal/cancellation/deadline and key rotation; scanner tests cover fail-closed protocol/admission with no storage effects. Actual ClamAV/browser/PDF evidence is separate from protocol fixtures. Preserve all cumulative gates. Refresh the D11 guidance record for crypto/engine/renderer changes or security findings; never silently download executable policy into a site.
