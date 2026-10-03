import { h, render, Fragment } from "preact";
import { useState, useEffect, useRef } from "preact/hooks";
import { Button, Field, Notice, Disclosure, SelectField } from "./ui.jsx";
const clone = (v) => JSON.parse(JSON.stringify(v));
const kinds = [
  "section",
  "grid",
  "row",
  "heading",
  "text",
  "body",
  "image",
  "form",
  "link",
  "navigation",
  "component",
  "collection",
  "repeater",
  "condition",
  "accordion",
  "tabs",
  "gallery",
  "carousel",
];
const fieldKinds = [
  "string",
  "number",
  "boolean",
  "media",
  "relationship",
  "object",
  "group",
  "repeater",
  "gallery",
  "flexible",
];
const node = (kind) => ({
  id: "node-" + crypto.randomUUID().slice(0, 8),
  kind,
  children: [],
  style: {},
});
const own = (o, k) => Object.hasOwn(o, k);
function Scalar({ label, value, onChange, type = "text", ...rest }) {
  return (
    <Field
      label={label}
      type={type}
      value={value ?? ""}
      onInput={(e) => onChange(e.currentTarget.value)}
      {...rest}
    />
  );
}
function Binding({ label, value, onChange }) {
  const bound = value && typeof value === "object";
  return (
    <div class="binding">
      <label>
        {label}
        <select
          aria-label={label}
          value={
            bound
              ? "bind"
              : typeof value === "number"
                ? "number"
                : typeof value === "boolean"
                  ? "boolean"
                  : "literal"
          }
          onChange={(e) =>
            onChange(
              e.currentTarget.value === "bind"
                ? { bind: "site.title" }
                : e.currentTarget.value === "number"
                  ? 0
                  : e.currentTarget.value === "boolean"
                    ? false
                    : "",
            )
          }
        >
          <option value="literal">Text literal</option>
          <option value="number">Number literal</option>
          <option value="boolean">Boolean literal</option>
          <option value="bind">Dynamic binding</option>
        </select>
      </label>
      {!bound && typeof value === "boolean" ? (
        <input
          type="checkbox"
          aria-label={label + " value"}
          checked={value}
          onChange={(e) => onChange(e.currentTarget.checked)}
        />
      ) : (
        <input
          type={!bound && typeof value === "number" ? "number" : "text"}
          aria-label={label + " value"}
          value={bound ? value.bind : (value ?? "")}
          placeholder={bound ? "post.fields.subtitle" : "Text"}
          onInput={(e) =>
            onChange(
              bound
                ? { ...value, bind: e.currentTarget.value }
                : typeof value === "number"
                  ? e.currentTarget.value === ""
                    ? null
                    : Number(e.currentTarget.value)
                  : e.currentTarget.value,
            )
          }
        />
      )}
      {bound && (
        <Scalar
          label="Fallback"
          value={value.fallback}
          onChange={(v) => onChange({ ...value, fallback: v })}
        />
      )}
    </div>
  );
}
function Condition({ value, onChange }) {
  const op = value?.op || "truthy";
  return (
    <div>
      <label>
        Condition operator
        <select
          aria-label="Condition operator"
          value={op}
          onChange={(e) =>
            onChange(
              e.currentTarget.value === "truthy"
                ? { bind: "post.fields.featured" }
                : {
                    op: e.currentTarget.value,
                    left: { bind: "post.fields.featured" },
                    right: true,
                  },
            )
          }
        >
          {["truthy", "eq", "ne", "gt", "lt"].map((v) => (
            <option>{v}</option>
          ))}
        </select>
      </label>
      {op === "truthy" ? (
        <Binding label="Condition" value={value} onChange={onChange} />
      ) : ["eq", "ne", "gt", "lt"].includes(op) ? (
        <>
          <Binding
            label="Condition left"
            value={value.left}
            onChange={(v) => onChange({ ...value, left: v })}
          />
          <Binding
            label="Condition right"
            value={value.right}
            onChange={(v) => onChange({ ...value, right: v })}
          />
        </>
      ) : (
        <p>Nested {op} condition; edit it in the validated package editor.</p>
      )}
    </div>
  );
}
function Outline({ root, path, selected, onSelect }) {
  return (
    <div class="outline">
      <Button
        class={"quiet " + (selected === path ? "selected" : "")}
        onClick={() => onSelect(path)}
      >
        {root.kind} <small>{root.id}</small>
      </Button>
      {(root.children || []).map((c, i) => (
        <Outline
          key={c.id}
          root={c}
          path={path + ".children." + i}
          selected={selected}
          onSelect={onSelect}
        />
      ))}
    </div>
  );
}
const at = (obj, path) => path.split(".").reduce((v, k) => v?.[k], obj);
function Fields({ definition, onChange, groups, models }) {
  return (
    <div class="field-definitions">
      {Object.entries(definition).map(([id, f]) => (
        <div class="field-definition" key={id}>
          <strong>{id}</strong>
          <select
            aria-label={"Type " + id}
            value={f.kind}
            onChange={(e) =>
              onChange({
                ...definition,
                [id]: { kind: e.currentTarget.value, label: f.label || id },
              })
            }
          >
            {fieldKinds.map((k) => (
              <option>{k}</option>
            ))}
          </select>
          <Scalar
            label="Label"
            value={f.label}
            onChange={(v) =>
              onChange({ ...definition, [id]: { ...f, label: v } })
            }
          />
          <label>
            <input
              type="checkbox"
              checked={f.required || false}
              onChange={(e) =>
                onChange({
                  ...definition,
                  [id]: { ...f, required: e.currentTarget.checked },
                })
              }
            />{" "}
            Required
          </label>
          {f.kind === "relationship" && (
            <label>
              Target model
              <select
                value={f.target || ""}
                onChange={(e) =>
                  onChange({
                    ...definition,
                    [id]: { ...f, target: e.currentTarget.value },
                  })
                }
              >
                <option value="">Choose…</option>
                {Object.keys(models).map((m) => (
                  <option>{m}</option>
                ))}
              </select>
            </label>
          )}
          {["object", "repeater"].includes(f.kind) && (
            <Fields
              definition={f.fields || {}}
              groups={groups}
              models={models}
              onChange={(v) =>
                onChange({ ...definition, [id]: { ...f, fields: v } })
              }
            />
          )}
          {f.kind === "group" && (
            <label>
              Reusable group
              <select
                value={f.group || ""}
                onChange={(e) =>
                  onChange({
                    ...definition,
                    [id]: { ...f, group: e.currentTarget.value },
                  })
                }
              >
                <option value="">Choose…</option>
                {Object.keys(groups).map((g) => (
                  <option>{g}</option>
                ))}
              </select>
            </label>
          )}
          {f.kind === "flexible" && (
            <div>
              {Object.entries(f.variants || {}).map(([variant, group]) => (
                <label>
                  {variant}
                  <select
                    value={group}
                    onChange={(e) =>
                      onChange({
                        ...definition,
                        [id]: {
                          ...f,
                          variants: {
                            ...f.variants,
                            [variant]: e.currentTarget.value,
                          },
                        },
                      })
                    }
                  >
                    <option value="">Choose group…</option>
                    {Object.keys(groups).map((g) => (
                      <option>{g}</option>
                    ))}
                  </select>
                </label>
              ))}
              <AddName
                label="Add variant"
                onAdd={(variant) =>
                  onChange({
                    ...definition,
                    [id]: { ...f, variants: { ...f.variants, [variant]: "" } },
                  })
                }
              />
            </div>
          )}
          {["repeater", "gallery", "flexible"].includes(f.kind) && (
            <Scalar
              label="Maximum items"
              type="number"
              min="1"
              max="50"
              value={f.max_items || 20}
              onChange={(v) =>
                onChange({
                  ...definition,
                  [id]: { ...f, max_items: Number(v) },
                })
              }
            />
          )}
          <Button
            class="quiet"
            onClick={() => {
              const next = { ...definition };
              delete next[id];
              onChange(next);
            }}
          >
            Remove field
          </Button>
        </div>
      ))}
      <AddName
        label="Add field"
        onAdd={(id) => {
          if (!own(definition, id))
            onChange({ ...definition, [id]: { kind: "string", label: id } });
        }}
      />
    </div>
  );
}
function AddName({ label, onAdd }) {
  const [id, set] = useState("");
  return (
    <div class="toolbar">
      <input
        aria-label={label + " identifier"}
        placeholder="identifier"
        value={id}
        onInput={(e) => set(e.currentTarget.value)}
      />
      <Button
        class="secondary"
        onClick={() => {
          if (
            /^[a-z][a-z0-9_-]{0,63}$/.test(id) &&
            !["constructor", "prototype", "__proto__"].includes(id)
          ) {
            onAdd(id);
            set("");
          }
        }}
      >
        {label}
      </Button>
    </div>
  );
}
function Values({ fields, value, onChange, state, groups }) {
  return (
    <div>
      {Object.entries(fields).map(([id, f]) => {
        const v = value?.[id],
          change = (v) => onChange({ ...value, [id]: v }),
          label = f.label || id;
        const children = f.group ? groups[f.group] : f.fields || {};
        return (
          <div class="typed-value" key={id}>
            {f.kind === "boolean" ? (
              <label>
                <input
                  type="checkbox"
                  checked={!!v}
                  onChange={(e) => change(e.currentTarget.checked)}
                />
                {label}
              </label>
            ) : ["media", "relationship"].includes(f.kind) ? (
              <label>
                {label}
                <select
                  value={v || ""}
                  onChange={(e) => change(e.currentTarget.value || null)}
                >
                  <option value="">None</option>
                  {(f.kind === "media"
                    ? state.media
                    : state.posts.filter((p) => p.kind === f.target)
                  ).map((p) => (
                    <option value={p.id}>
                      {p.label || p.title} ({p.visibility || p.status})
                    </option>
                  ))}
                </select>
              </label>
            ) : ["object", "group"].includes(f.kind) ? (
              <fieldset>
                <legend>{label}</legend>
                <Values
                  fields={children || {}}
                  value={v || {}}
                  onChange={change}
                  state={state}
                  groups={groups}
                />
              </fieldset>
            ) : f.kind === "repeater" ? (
              <fieldset>
                <legend>{label}</legend>
                {(v || []).map((row, i) => (
                  <div>
                    <Values
                      fields={children}
                      value={row}
                      state={state}
                      groups={groups}
                      onChange={(next) =>
                        change(v.map((x, j) => (j === i ? next : x)))
                      }
                    />
                    <Button
                      class="quiet"
                      onClick={() => change(v.filter((_, j) => j !== i))}
                    >
                      Remove row
                    </Button>
                  </div>
                ))}
                <Button
                  class="secondary"
                  onClick={() => change([...(v || []), {}])}
                  disabled={(v || []).length >= (f.max_items || 20)}
                >
                  Add row
                </Button>
              </fieldset>
            ) : f.kind === "gallery" ? (
              <fieldset>
                <legend>{label}</legend>
                {(v || []).map((id, i) => (
                  <div class="toolbar">
                    <span>
                      {state.media.find((m) => m.id === id)?.label || id}
                    </span>
                    <Button
                      class="quiet"
                      onClick={() => change(v.filter((_, j) => j !== i))}
                    >
                      Remove
                    </Button>
                  </div>
                ))}
                <select
                  aria-label={"Add " + label}
                  value=""
                  onChange={(e) =>
                    e.currentTarget.value &&
                    change([...(v || []), e.currentTarget.value])
                  }
                >
                  <option value="">Add image…</option>
                  {state.media.map((m) => (
                    <option value={m.id}>{m.label}</option>
                  ))}
                </select>
              </fieldset>
            ) : f.kind === "flexible" ? (
              <fieldset>
                <legend>{label}</legend>
                {(v || []).map((row, i) => (
                  <section>
                    <strong>{row.type}</strong>
                    <Values
                      fields={groups[f.variants[row.type]] || {}}
                      value={row.values}
                      state={state}
                      groups={groups}
                      onChange={(next) =>
                        change(
                          v.map((x, j) =>
                            j === i ? { ...x, values: next } : x,
                          ),
                        )
                      }
                    />
                    <Button
                      class="quiet"
                      onClick={() => change(v.filter((_, j) => j !== i))}
                    >
                      Remove section
                    </Button>
                  </section>
                ))}
                <select
                  value=""
                  aria-label={"Add " + label}
                  onChange={(e) =>
                    e.currentTarget.value &&
                    change([
                      ...(v || []),
                      { type: e.currentTarget.value, values: {} },
                    ])
                  }
                >
                  <option value="">Add section…</option>
                  {Object.keys(f.variants || {}).map((t) => (
                    <option>{t}</option>
                  ))}
                </select>
              </fieldset>
            ) : (
              <Scalar
                label={label}
                value={v}
                type={f.kind === "number" ? "number" : "text"}
                required={f.required}
                onChange={(v) =>
                  change(
                    f.kind === "number" ? (v === "" ? null : Number(v)) : v,
                  )
                }
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
function App() {
  const [state, setState] = useState(null),
    [id, setId] = useState(""),
    [pkg, setPkg] = useState(null),
    [tab, setTab] = useState("compose"),
    [template, setTemplate] = useState("home"),
    [selected, setSelected] = useState("templates.home"),
    [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false),
    [preview, setPreview] = useState(0),
    [mobile, setMobile] = useState(false),
    [post, setPost] = useState(""),
    [dirty, setDirty] = useState(false),
    [raw, setRaw] = useState(""),
    [blocked, setBlocked] = useState(false),
    [model, setModel] = useState("post"),
    [common, setCommon] = useState(null),
    [models, setModels] = useState(null),
    [options, setOptions] = useState({});
  const csrf = document.querySelector("#builder").dataset.csrf;
  const version = useRef(0),
    flight = useRef(false),
    generation = useRef(0);
  async function api(path, body) {
    const r = await fetch(path, {
      method: body ? "POST" : "GET",
      headers: body ? { "Content-Type": "application/json" } : {},
      body: body ? JSON.stringify({ ...body, csrf }) : undefined,
    });
    const data = await r.json();
    if (!r.ok) throw new Error(data.error || "Request failed");
    return data;
  }
  async function reload(chosen) {
    const requested = chosen || id;
    const s = await api(
      "/api/admin/design" +
        (requested ? "?theme=" + encodeURIComponent(requested) : ""),
    );
    setState(s);
    setCommon(clone(s.registry.common));
    setModels(clone(s.registry.models));
    setOptions(clone(s.design.draft_options));
    const t =
      s.themes.find((t) => t.id === (chosen || id || s.active)) || s.themes[0];
    setId(t.id);
    setPkg(clone(t.package));
    version.current = t.version;
    setDirty(false);
    setBlocked(false);
    setRaw(JSON.stringify(t.package, null, 2));
    setPreview((x) => x + 1);
  }
  useEffect(() => {
    reload().catch((e) => setMessage(e.message));
  }, []);
  async function action(fn) {
    if (flight.current) return;
    flight.current = true;
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      setMessage(e.message);
      setBlocked(true);
    } finally {
      flight.current = false;
      setBusy(false);
    }
  }
  async function save(publish = false) {
    const snapshot = generation.current;
    const next = await api("/api/admin/design/" + id, {
      version: version.current,
      package: pkg,
      publish,
    });
    version.current = next.version;
    if (snapshot === generation.current) setDirty(false);
    setBlocked(false);
    setPreview((x) => x + 1);
    setMessage(
      publish
        ? "Theme published. Activate it to use it on the website."
        : "Draft saved. Live website is unchanged.",
    );
    const s = await api("/api/admin/design");
    setState(s);
  }
  useEffect(() => {
    if (!dirty || !pkg || busy || blocked) return;
    const timer = setTimeout(() => action(() => save(false)), 1800);
    return () => clearTimeout(timer);
  }, [pkg, dirty, busy, blocked]);
  function update(fn) {
    generation.current++;
    setBlocked(false);
    const next = clone(pkg);
    fn(next);
    setPkg(next);
    setDirty(true);
    setRaw(JSON.stringify(next, null, 2));
    setMessage("Unsaved draft");
  }
  if (!state || !pkg)
    return <p role="status">{message || "Loading studio…"}</p>;
  const current = at(pkg, selected);
  const schema = common;
  return (
    <>
      <div class="studio-toolbar">
        <label>
          Theme
          <select
            aria-label="Theme package"
            value={id}
            onChange={(e) => {
              const nextId = e.currentTarget.value;
              action(async () => {
                if (dirty) await save(false);
                await reload(nextId);
              });
            }}
          >
            {state.themes.map((t) => (
              <option value={t.id}>
                {t.name}
                {state.active === t.id ? " · live" : ""}
              </option>
            ))}
          </select>
        </label>
        <Button busy={busy} onClick={() => action(() => save(false))}>
          Save draft
        </Button>
        <Button
          variant="secondary"
          busy={busy}
          onClick={() => action(() => save(true))}
        >
          Publish theme
        </Button>
        <Button
          class="secondary"
          disabled={busy || dirty}
          onClick={() =>
            action(async () => {
              await api("/api/admin/design/" + id + "/activate", {});
              setMessage("Theme activated");
              await reload(id);
            })
          }
        >
          Activate
        </Button>
        <Button
          class="secondary"
          onClick={() => {
            const blob = new Blob([JSON.stringify(pkg, null, 2)], {
              type: "application/json",
            });
            const a = document.createElement("a");
            a.href = URL.createObjectURL(blob);
            a.download = id + ".json";
            a.click();
            URL.revokeObjectURL(a.href);
          }}
        >
          Export package
        </Button>
        <label class="file-button">
          Import package
          <input
            type="file"
            accept="application/json"
            onChange={(e) =>
              action(async () => {
                const file = e.currentTarget.files[0];
                if (!file) return;
                const raw = await file.text();
                const p = JSON.parse(raw);
                const name = "import-" + crypto.randomUUID().slice(0, 8);
                await api("/api/admin/design/" + name, {
                  version: 0,
                  package: p,
                  publish: false,
                });
                await reload(name);
                setMessage(
                  "Imported as a draft; inspect dependencies before publication.",
                );
              })
            }
          />
        </label>
      </div>
      <Notice error={blocked}>
        {message ||
          "Drafts are private. Theme and shared options publish independently."}
      </Notice>
      <nav class="studio-tabs" aria-label="Studio tools">
        {["compose", "models", "options", "history", "package"].map((t) => (
          <Button
            class={tab === t ? "" : "secondary"}
            aria-current={tab === t ? "page" : undefined}
            onClick={() => setTab(t)}
          >
            {t}
          </Button>
        ))}
      </nav>
      {tab === "compose" && (
        <div class="studio">
          <aside class="panel">
            <label>
              Template
              <select
                aria-label="Template"
                value={selected.startsWith("components.") ? selected : template}
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  if (v.startsWith("components.")) setSelected(v);
                  else {
                    setTemplate(v);
                    setSelected(
                      v === "header" || v === "footer" ? v : "templates." + v,
                    );
                  }
                }}
              >
                <optgroup label="Templates">
                  {Object.keys(pkg.templates).map((t) => (
                    <option>{t}</option>
                  ))}
                  <option>header</option>
                  <option>footer</option>
                </optgroup>
                <optgroup label="Reusable components">
                  {Object.keys(pkg.components).map((c) => (
                    <option value={"components." + c + ".root"}>{c}</option>
                  ))}
                </optgroup>
              </select>
            </label>
            <AddName
              label="Add component"
              onAdd={(name) =>
                update((p) => {
                  p.components[name] = {
                    parameters: {},
                    root: node("section"),
                  };
                  setSelected("components." + name + ".root");
                })
              }
            />
            <AddName
              label="Add model template"
              onAdd={(name) => {
                if (own(models, name))
                  update((p) => {
                    p.templates[name] = node("section");
                    setTemplate(name);
                    setSelected("templates." + name);
                  });
              }}
            />
            {current && (
              <Outline
                root={at(
                  pkg,
                  selected.startsWith("components.")
                    ? selected.split(".").slice(0, 3).join(".")
                    : selected === "header" || selected === "footer"
                      ? selected
                      : "templates." + template,
                )}
                path={
                  selected.startsWith("components.")
                    ? selected.split(".").slice(0, 3).join(".")
                    : selected === "header" || selected === "footer"
                      ? selected
                      : "templates." + template
                }
                selected={selected}
                onSelect={setSelected}
              />
            )}
            <Disclosure summary="Design tokens" collapseOnNarrow>
              {Object.keys(pkg.tokens).map((k) => (
                <Scalar
                  label={k}
                  type={k === "font" ? "text" : "color"}
                  value={pkg.tokens[k]}
                  onChange={(v) => update((p) => (p.tokens[k] = v))}
                />
              ))}
            </Disclosure>
          </aside>
          <section class="panel preview-panel">
            <div class="toolbar">
              <Button class="secondary" onClick={() => setMobile((v) => !v)}>
                {mobile ? "Desktop preview" : "Mobile preview"}
              </Button>
              <label>
                Preview content
                <select
                  aria-label="Preview content"
                  value={post}
                  onChange={(e) => setPost(e.currentTarget.value)}
                >
                  <option value="">Home</option>
                  {state.posts.map((p) => (
                    <option value={p.id}>{p.title}</option>
                  ))}
                </select>
              </label>
              <span>Server-rendered draft</span>
            </div>
            <iframe
              title="Website draft preview"
              class={mobile ? "preview-mobile" : "preview-desktop"}
              sandbox="allow-same-origin"
              src={
                "/admin/design/" +
                id +
                "/preview?template=" +
                encodeURIComponent(
                  ["header", "footer"].includes(template) ? "home" : template,
                ) +
                "&post=" +
                post +
                "&v=" +
                preview
              }
            />
          </section>
          <aside class="panel properties">
            {current && (
              <>
                <h3>Selected node</h3>
                <Scalar
                  label="Node identifier"
                  value={current.id}
                  onChange={(v) => update((p) => (at(p, selected).id = v))}
                />
                <label>
                  Node type
                  <select
                    aria-label="Node type"
                    value={current.kind}
                    onChange={(e) =>
                      update(
                        (p) => (at(p, selected).kind = e.currentTarget.value),
                      )
                    }
                  >
                    {kinds.map((k) => (
                      <option>{k}</option>
                    ))}
                  </select>
                </label>
                {[
                  "heading",
                  "text",
                  "link",
                  "image",
                  "accordion",
                  "tabs",
                ].includes(current.kind) && (
                  <Binding
                    label="Text"
                    value={current.text}
                    onChange={(v) => update((p) => (at(p, selected).text = v))}
                  />
                )}
                {current.kind === "form" && (
                  <SelectField
                    label="Published form"
                    value={current.text || ""}
                    onChange={(event) =>
                      update((p) => {
                        at(p, selected).text = event.currentTarget.value;
                      })
                    }
                  >
                    <option value="">Choose a published form…</option>
                    {(state.forms || []).map((form) => (
                      <option value={form.id}>{form.title}</option>
                    ))}
                  </SelectField>
                )}
                {current.kind === "link" && (
                  <Binding
                    label="Destination"
                    value={current.href}
                    onChange={(v) => update((p) => (at(p, selected).href = v))}
                  />
                )}
                {["image", "gallery", "carousel"].includes(current.kind) && (
                  <Binding
                    label="Media"
                    value={current.image}
                    onChange={(v) => update((p) => (at(p, selected).image = v))}
                  />
                )}
                {current.kind === "condition" && (
                  <Condition
                    value={current.condition}
                    onChange={(v) =>
                      update((p) => (at(p, selected).condition = v))
                    }
                  />
                )}
                {["collection", "repeater"].includes(current.kind) && (
                  <>
                    <Scalar
                      label="Source"
                      value={current.source}
                      onChange={(v) =>
                        update((p) => (at(p, selected).source = v))
                      }
                    />
                    <Scalar
                      label="Item limit"
                      type="number"
                      min="1"
                      max="50"
                      value={current.limit || 12}
                      onChange={(v) =>
                        update((p) => (at(p, selected).limit = Number(v)))
                      }
                    />
                  </>
                )}
                {current.kind === "component" && (
                  <>
                    <label>
                      Component
                      <select
                        aria-label="Component"
                        value={current.component || ""}
                        onChange={(e) =>
                          update((p) => {
                            const n = at(p, selected);
                            n.component = e.currentTarget.value;
                            n.arguments = Object.fromEntries(
                              Object.keys(
                                p.components[n.component].parameters || {},
                              ).map((k) => [k, ""]),
                            );
                          })
                        }
                      >
                        <option value="">Choose…</option>
                        {Object.keys(pkg.components).map((c) => (
                          <option>{c}</option>
                        ))}
                      </select>
                    </label>
                    {Object.keys(
                      pkg.components[current.component]?.parameters || {},
                    ).map((k) => (
                      <Binding
                        label={"Parameter " + k}
                        value={current.arguments?.[k]}
                        onChange={(v) =>
                          update((p) => (at(p, selected).arguments[k] = v))
                        }
                      />
                    ))}
                  </>
                )}
                {selected.startsWith("components.") && (
                  <>
                    <h4>Component parameters</h4>
                    {Object.entries(
                      pkg.components[selected.split(".")[1]].parameters || {},
                    ).map(([k, t]) => (
                      <label>
                        {k}
                        <select
                          aria-label={"Parameter type " + k}
                          value={t}
                          onChange={(e) =>
                            update(
                              (p) =>
                                (p.components[
                                  selected.split(".")[1]
                                ].parameters[k] = e.currentTarget.value),
                            )
                          }
                        >
                          {[
                            "string",
                            "number",
                            "boolean",
                            "media",
                            "relationship",
                          ].map((type) => (
                            <option>{type}</option>
                          ))}
                        </select>
                      </label>
                    ))}
                    <AddName
                      label="Add parameter"
                      onAdd={(name) =>
                        update(
                          (p) =>
                            (p.components[selected.split(".")[1]].parameters[
                              name
                            ] = "string"),
                        )
                      }
                    />
                  </>
                )}
                {current.kind === "heading" && (
                  <Scalar
                    label="Heading level"
                    type="number"
                    min="1"
                    max="6"
                    value={current.level || 2}
                    onChange={(v) =>
                      update((p) => (at(p, selected).level = Number(v)))
                    }
                  />
                )}
                <h4>Responsive layout</h4>
                <label>
                  Layout
                  <select
                    aria-label="Layout"
                    value={current.style?.layout || ""}
                    onChange={(e) =>
                      update(
                        (p) =>
                          (at(p, selected).style = {
                            ...at(p, selected).style,
                            layout: e.currentTarget.value,
                          }),
                      )
                    }
                  >
                    {["", "stack", "grid", "row"].map((v) => (
                      <option value={v}>{v || "Default"}</option>
                    ))}
                  </select>
                </label>
                {["columns", "mobile_columns", "gap", "padding", "width"].map(
                  (k) => (
                    <Scalar
                      label={k.replaceAll("_", " ")}
                      type="number"
                      value={current.style?.[k] || 0}
                      onChange={(v) =>
                        update(
                          (p) =>
                            (at(p, selected).style = {
                              ...at(p, selected).style,
                              [k]: Number(v),
                            }),
                        )
                      }
                    />
                  ),
                )}
                <label>
                  Add child
                  <select
                    aria-label="Add child"
                    value=""
                    onChange={(e) => {
                      const kind = e.currentTarget.value;
                      update((p) => {
                        const n = at(p, selected);
                        n.children = [...(n.children || []), node(kind)];
                      });
                    }}
                  >
                    <option value="">Choose node…</option>
                    {kinds.map((k) => (
                      <option>{k}</option>
                    ))}
                  </select>
                </label>
                {selected.includes(".children.") && (
                  <div class="toolbar">
                    <Button
                      class="secondary"
                      onClick={() =>
                        update((p) => {
                          const parts = selected.split(".");
                          const index = Number(parts.pop());
                          parts.pop();
                          const parent = at(p, parts.join("."));
                          if (index > 0)
                            [
                              parent.children[index - 1],
                              parent.children[index],
                            ] = [
                              parent.children[index],
                              parent.children[index - 1],
                            ];
                          setSelected(
                            parts.join(".") +
                              ".children." +
                              Math.max(0, index - 1),
                          );
                        })
                      }
                    >
                      Move up
                    </Button>
                    <Button
                      class="secondary"
                      onClick={() =>
                        update((p) => {
                          const parts = selected.split(".");
                          const index = Number(parts.pop());
                          parts.pop();
                          at(p, parts.join(".")).children.splice(index, 1);
                          setSelected(parts.join("."));
                        })
                      }
                    >
                      Remove node
                    </Button>
                  </div>
                )}
              </>
            )}
          </aside>
        </div>
      )}
      {tab === "models" && (
        <section class="panel">
          <h2>Typed content</h2>
          <p>
            Definitions validate working and published data before saving. Add
            optional fields, populate records, then make them required.
          </p>
          <label>
            Content model
            <select
              aria-label="Content model"
              value={model}
              onChange={(e) => setModel(e.currentTarget.value)}
            >
              {Object.keys(models).map((m) => (
                <option>{m}</option>
              ))}
            </select>
          </label>
          <AddName
            label="Add model"
            onAdd={(name) => {
              setModels({
                ...models,
                [name]: {
                  label: name,
                  fields: {},
                  taxonomies: { category: "Categories", tag: "Tags" },
                },
              });
              setModel(name);
            }}
          />
          <Scalar
            label="Model label"
            value={models[model].label}
            onChange={(v) =>
              setModels({ ...models, [model]: { ...models[model], label: v } })
            }
          />
          <Fields
            definition={models[model].fields}
            groups={schema.groups}
            models={models}
            onChange={(v) =>
              setModels({ ...models, [model]: { ...models[model], fields: v } })
            }
          />
          <h3>Taxonomies</h3>
          {Object.entries(models[model].taxonomies).map(([id, label]) => (
            <Scalar
              label={id}
              value={label}
              onChange={(v) =>
                setModels({
                  ...models,
                  [model]: {
                    ...models[model],
                    taxonomies: { ...models[model].taxonomies, [id]: v },
                  },
                })
              }
            />
          ))}
          <AddName
            label="Add taxonomy"
            onAdd={(id) =>
              setModels({
                ...models,
                [model]: {
                  ...models[model],
                  taxonomies: { ...models[model].taxonomies, [id]: id },
                },
              })
            }
          />
          <Button
            onClick={() =>
              action(async () => {
                await api("/api/admin/models/" + model, {
                  version: state.model_versions[model] || 0,
                  definition: models[model],
                });
                await reload(id);
                setMessage("Model saved after existing-data validation");
              })
            }
          >
            Save model
          </Button>
          <h3>Common fields</h3>
          <Fields
            definition={schema.fields}
            groups={schema.groups}
            models={models}
            onChange={(v) => setCommon({ ...schema, fields: v })}
          />
          <h3>Reusable field groups</h3>
          {Object.entries(schema.groups).map(([id, fields]) => (
            <fieldset>
              <legend>{id}</legend>
              <Fields
                definition={fields}
                groups={schema.groups}
                models={models}
                onChange={(v) =>
                  setCommon({
                    ...schema,
                    groups: { ...schema.groups, [id]: v },
                  })
                }
              />
            </fieldset>
          ))}
          <AddName
            label="Add group"
            onAdd={(id) =>
              setCommon({ ...schema, groups: { ...schema.groups, [id]: {} } })
            }
          />
          <h3>Shared option definitions</h3>
          <Fields
            definition={schema.options}
            groups={schema.groups}
            models={models}
            onChange={(v) => setCommon({ ...schema, options: v })}
          />
          <Button
            onClick={() =>
              action(async () => {
                await api("/api/admin/schema", {
                  version: state.design.version,
                  definition: common,
                });
                await reload(id);
                setMessage("Shared definitions saved");
              })
            }
          >
            Save shared definitions
          </Button>
        </section>
      )}
      {tab === "options" && (
        <section class="panel">
          <h2>Shared site options</h2>
          <Values
            fields={state.registry.common.options}
            groups={state.registry.common.groups}
            value={options}
            state={state}
            onChange={setOptions}
          />
          <div class="toolbar">
            {[false, true].map((publish) => (
              <Button
                onClick={() =>
                  action(async () => {
                    await api("/api/admin/options", {
                      version: state.design.version,
                      values: options,
                      publish,
                    });
                    await reload(id);
                    setMessage(
                      publish ? "Options published" : "Option draft saved",
                    );
                  })
                }
              >
                {publish ? "Publish options" : "Save option draft"}
              </Button>
            ))}
          </div>
        </section>
      )}
      {tab === "history" && (
        <section class="panel">
          <h2>Theme revisions</h2>
          <p>Restore creates a draft. Review and publish it deliberately.</p>
          {state.revisions
            .filter((r) => r.theme === id)
            .map((r) => (
              <div class="toolbar">
                <span>
                  Version {r.version}
                  {r.published ? " · published" : " · draft"}
                </span>
                <Button
                  class="secondary"
                  onClick={() =>
                    action(async () => {
                      await api(
                        "/api/admin/design/" + id + "/restore/" + r.version,
                        { version: version.current },
                      );
                      await reload(id);
                      setMessage("Prior version restored as a draft");
                    })
                  }
                >
                  Restore draft
                </Button>
              </div>
            ))}
        </section>
      )}
      {tab === "package" && (
        <section class="panel">
          <h2>Portable package</h2>
          <p>
            Advanced structured editing uses the same server validation as the
            visual studio.
          </p>
          <textarea
            aria-label="Theme JSON"
            rows="28"
            value={raw}
            onInput={(e) => setRaw(e.currentTarget.value)}
          />
          <Button
            onClick={() =>
              action(async () => {
                const packageValue = JSON.parse(raw);
                await api("/api/admin/design/" + id, {
                  version: version.current,
                  package: packageValue,
                  publish: false,
                });
                await reload(id);
                setMessage("Validated package saved as a draft");
              })
            }
          >
            Validate and save package
          </Button>
        </section>
      )}
    </>
  );
}
const studio = document.querySelector("#builder");
if (studio) render(<App />, studio);
const form = document.querySelector("[data-editor]");
if (form) {
  fetch("/api/admin/design")
    .then((r) => r.json())
    .then((state) => {
      if (!state.registry) return;
      const select = form.elements.kind;
      const originalKind = select.value;
      select.replaceChildren(
        ...Object.entries(state.registry.models).map(([id, m]) => {
          const o = new Option(m.label, id);
          o.selected = id === originalKind;
          return o;
        }),
      );
      const raw = form.elements.fields;
      const mount = document.createElement("div");
      raw.closest("label").before(mount);
      function ContentFields() {
        const [value, setValue] = useState(JSON.parse(raw.value || "{}")),
          [kind, setKind] = useState(select.value);
        useEffect(() => {
          const recovered = () => setValue(JSON.parse(raw.value || "{}"));
          form.addEventListener("wpalt:recovery", recovered);
          return () => form.removeEventListener("wpalt:recovery", recovered);
        }, []);
        useEffect(() => {
          const fn = () => setKind(select.value);
          select.addEventListener("change", fn);
          fn();
          return () => select.removeEventListener("change", fn);
        }, []);
        return (
          <fieldset>
            <legend>Structured content fields</legend>
            <Values
              fields={{
                ...state.registry.common.fields,
                ...state.registry.models[kind].fields,
              }}
              groups={state.registry.common.groups}
              state={state}
              value={value}
              onChange={(v) => {
                setValue(v);
                raw.value = JSON.stringify(v);
                raw.dispatchEvent(new Event("input", { bubbles: true }));
              }}
            />
          </fieldset>
        );
      }
      render(<ContentFields />, mount);
      const taxonomyRaw = form.elements.taxonomies;
      const taxonomyMount = document.createElement("div");
      taxonomyRaw.closest("label").before(taxonomyMount);
      function Taxonomies() {
        const [kind, setKind] = useState(select.value),
          [values, setValues] = useState(JSON.parse(taxonomyRaw.value || "{}"));
        useEffect(() => {
          const recovered = () =>
            setValues(JSON.parse(taxonomyRaw.value || "{}"));
          form.addEventListener("wpalt:recovery", recovered);
          return () => form.removeEventListener("wpalt:recovery", recovered);
        }, []);
        useEffect(() => {
          const fn = () => setKind(select.value);
          select.addEventListener("change", fn);
          fn();
          return () => select.removeEventListener("change", fn);
        }, []);
        return (
          <div>
            {Object.entries(state.registry.models[kind].taxonomies)
              .filter(([id]) => id !== "category" && id !== "tag")
              .map(([id, label]) => (
                <Scalar
                  label={label}
                  value={(values[id] || []).join(", ")}
                  onChange={(v) => {
                    const next = {
                      ...values,
                      [id]: v
                        .split(",")
                        .map((x) => x.trim())
                        .filter(Boolean),
                    };
                    setValues(next);
                    taxonomyRaw.value = JSON.stringify(next);
                    taxonomyRaw.dispatchEvent(
                      new Event("input", { bubbles: true }),
                    );
                  }}
                />
              ))}
          </div>
        );
      }
      render(<Taxonomies />, taxonomyMount);
    })
    .catch(() => {});
}
