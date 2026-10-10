import { h, Fragment } from "preact";
import { useState, useEffect } from "preact/hooks";
import { Button, Field, SelectField, Disclosure, Notice } from "./ui.jsx";
function dependencies(source) {
  const media = new Map(),
    fonts = new Set(),
    globals = new Set();
  let visited = 0;
  function walk(v, depth = 0) {
    if (++visited > 2000 || depth > 32) return;
    if (!v || typeof v !== "object") return;
    if (v.settings) {
      const s = v.settings;
      for (const reference of Object.values(
        s.__globals__ &&
          typeof s.__globals__ === "object" &&
          !Array.isArray(s.__globals__)
          ? s.__globals__
          : {},
      ))
        if (
          typeof reference === "string" &&
          reference.length <= 512 &&
          globals.size < 128
        )
          globals.add(reference);
      if (
        typeof s.typography_font_family === "string" &&
        s.typography_font_family.length <= 300 &&
        fonts.size < 32
      )
        fonts.add(s.typography_font_family);
      for (const image of [
        s.image,
        ...(Array.isArray(s.wp_gallery) ? s.wp_gallery : []),
        ...(Array.isArray(s.carousel) ? s.carousel : []),
      ]) {
        if (!image) continue;
        const src = image.$$type === "image" ? image.value?.src?.value : image;
        const id = src?.id?.value ?? src?.id,
          url = src?.url?.value ?? src?.url;
        const key = id ? String(id) : url;
        if (typeof key === "string" && key.length <= 2000 && media.size < 128)
          media.set(key, String(url || key).slice(0, 200));
      }
    }
    for (const child of (Array.isArray(v.content) ? v.content : []).concat(
      Array.isArray(v.elements) ? v.elements : [],
    ))
      walk(child, depth + 1);
  }
  walk(source);
  return { media: [...media], fonts: [...fonts], globals: [...globals] };
}
export function ElementorImport({
  id,
  version,
  pkg,
  state,
  dirty,
  busy,
  api,
  action,
  reload,
}) {
  const [request, setRequest] = useState(null),
    [review, setReview] = useState(null),
    [ack, setAck] = useState(false);
  useEffect(() => {
    setReview(null);
    setAck(false);
  }, [id, version]);
  function change(patch) {
    setRequest({ ...request, ...patch });
    setReview(null);
    setAck(false);
  }
  const refs = request
    ? dependencies(request.source)
    : { media: [], fonts: [], globals: [] };
  return (
    <Disclosure summary="Review an Elementor design import">
      <p>
        Import an export as editable native composition. Review unsupported
        settings and map images/fonts to admitted local assets. Saving creates a
        private draft; publication remains a separate action.
      </p>
      {dirty && (
        <Notice>
          Save your current theme edits before reviewing an import.
        </Notice>
      )}
      <label class="file-button">
        Choose Elementor JSON export
        <input
          type="file"
          accept="application/json,.json"
          disabled={dirty || busy}
          onChange={(e) => {
            const file = e.currentTarget.files[0];
            e.currentTarget.value = "";
            if (!file) return;
            action(async () => {
              if (file.size > 2 * 1024 * 1024)
                throw new Error("Elementor exports are limited to 2 MiB.");
              const source = JSON.parse(await file.text());
              setRequest({
                source,
                component: "imported-" + crypto.randomUUID().slice(0, 8),
                target: "component",
                content_id: "",
                content_kind: "",
                media: {},
                fonts: {},
              });
              setReview(null);
              setAck(false);
            });
          }}
        />
      </label>
      {request && (
        <Fragment>
          <Field
            label="New reusable component name"
            value={request.component}
            disabled={busy}
            onInput={(e) => change({ component: e.currentTarget.value })}
          />
          <SelectField
            label="Import placement"
            value={request.target}
            disabled={busy}
            onChange={(e) =>
              change({
                target: e.currentTarget.value,
                content_id: "",
                content_kind: "",
              })
            }
          >
            <option value="component">Reusable component only</option>
            <option value="content">One specific content item</option>
            <option value="home">Replace home template</option>
            <option value="header">Replace shared header</option>
            <option value="footer">Replace shared footer</option>
          </SelectField>
          {request.target === "component" && (
            <p>
              After importing, select the component in Studio and place it into
              a template to preview it.
            </p>
          )}
          {["home", "header", "footer"].includes(request.target) && (
            <Notice>
              This placement replaces a shared region in the draft. Preview
              affected pages before publishing.
            </Notice>
          )}
          {request.target === "content" && (
            <SelectField
              label="Content destination"
              value={request.content_id}
              disabled={busy}
              onChange={(e) => {
                const post = state.posts.find(
                  (p) => p.id === e.currentTarget.value,
                );
                change({
                  content_id: post?.id || "",
                  content_kind: post?.kind || "",
                });
              }}
            >
              <option value="">Choose content</option>
              {state.posts.map((p) => (
                <option value={p.id}>
                  {p.title} · {p.kind} · {p.status}
                </option>
              ))}
            </SelectField>
          )}
          {refs.media.map(([key, label]) => (
            <SelectField
              label={"Local image for " + label}
              value={request.media[key] || ""}
              disabled={busy}
              onChange={(e) => {
                const media = { ...request.media };
                if (e.currentTarget.value) media[key] = e.currentTarget.value;
                else delete media[key];
                change({ media });
              }}
            >
              <option value="">Leave unmapped and report loss</option>
              {state.media.map((m) => (
                <option value={m.id}>
                  {m.label} · {m.visibility}
                </option>
              ))}
            </SelectField>
          ))}
          {refs.fonts.map((name) => (
            <SelectField
              label={"Local font for " + name}
              value={request.fonts[name] || ""}
              disabled={busy}
              onChange={(e) => {
                const fonts = { ...request.fonts };
                if (e.currentTarget.value) fonts[name] = e.currentTarget.value;
                else delete fonts[name];
                change({ fonts });
              }}
            >
              <option value="">Leave unmapped and report loss</option>
              <option value="system">System sans-serif</option>
              <option value="serif">System serif</option>
              <option value="mono">System monospace</option>
              {Object.keys(pkg.fonts || {}).map((name) => (
                <option value={"local:" + name}>{name}</option>
              ))}
            </SelectField>
          ))}
          {refs.globals.map((reference) => (
            <SelectField
              label={"Native reusable style for " + reference}
              value={request.global_styles?.[reference] || ""}
              disabled={busy}
              onChange={(e) => {
                const global_styles = { ...request.global_styles };
                if (e.currentTarget.value)
                  global_styles[reference] = e.currentTarget.value;
                else delete global_styles[reference];
                change({ global_styles });
              }}
            >
              <option value="">Leave unresolved and report loss</option>
              {Object.keys(pkg.styles || {}).map((name) => (
                <option>{name}</option>
              ))}
            </SelectField>
          ))}
          <Button
            disabled={
              dirty ||
              busy ||
              (request.target === "content" && !request.content_id)
            }
            onClick={() =>
              action(async () => {
                const result = await api(
                  `/api/admin/design/${id}/elementor/review`,
                  { version, request },
                );
                setReview(result);
                setAck(false);
              })
            }
          >
            Review native mapping
          </Button>
          {review && (
            <section
              class="elementor-review"
              aria-label="Elementor import review"
            >
              <p>
                {review.report.elements} source elements ·{" "}
                {review.report.mappings.length} mappings ·{" "}
                {review.report.losses.length} reported losses. Live website
                remains unchanged.
              </p>
              <details>
                <summary>Mapped elements</summary>
                <ul>
                  {review.report.mappings.map((m) => (
                    <li>
                      {m.element}: {m.source_global || m.widget || "container"}{" "}
                      → {m.target || m.native_style}
                    </li>
                  ))}
                </ul>
              </details>
              <details open={review.report.losses.length > 0}>
                <summary>Unsupported or normalized source features</summary>
                <ul>
                  {review.report.losses.map((l) => (
                    <li>
                      <strong>{l.element}</strong>:{" "}
                      {l.code.replaceAll("_", " ")} · {JSON.stringify(l.detail)}
                    </li>
                  ))}
                </ul>
              </details>
              <label>
                <input
                  type="checkbox"
                  checked={ack}
                  disabled={busy}
                  onChange={(e) => setAck(e.currentTarget.checked)}
                />{" "}
                I reviewed the mapping and every reported loss.
              </label>
              <Button
                disabled={dirty || busy || !ack}
                onClick={() =>
                  action(async () => {
                    await api(`/api/admin/design/${id}/elementor/apply`, {
                      version,
                      request,
                      fingerprint: review.fingerprint,
                      acknowledge_losses: ack,
                    });
                    setRequest(null);
                    setReview(null);
                    setAck(false);
                    await reload(id);
                  })
                }
              >
                Import reviewed private draft
              </Button>
            </section>
          )}
        </Fragment>
      )}
    </Disclosure>
  );
}
