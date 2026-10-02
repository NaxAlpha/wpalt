import { h, render, Fragment } from "preact";
import { useEffect, useState, useRef } from "preact/hooks";
import {
  Button,
  Field,
  Notice,
  Disclosure,
  SelectField as Select,
} from "./ui.jsx";

async function request(url, body) {
  const response = await fetch(url, {
    method: body ? "POST" : "GET",
    headers: body ? { "Content-Type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
    credentials: "same-origin",
  });
  const result = await response.json().catch(() => ({
    error:
      response.status === 413
        ? "This form exceeds the 128 KiB request limit. Reduce the response size; your work is still here."
        : "The server could not complete the request. Your work is still here.",
  }));
  if (!response.ok)
    throw new Error(
      result.error ||
        "The request could not be completed. Your work is still here.",
    );
  return result;
}
function Designer({ host }) {
  const [state, setState] = useState(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState(false);
  const dirty = useRef(false);
  const endpoint = `/api/admin/forms/${host.dataset.id}`;
  useEffect(() => {
    let active = true;
    request(endpoint)
      .then((value) => {
        if (active) setState(value);
      })
      .catch((err) => {
        if (active) {
          setError(true);
          setNotice(err.message);
        }
      });
    const prevent = (event) => {
      if (dirty.current) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", prevent);
    return () => {
      active = false;
      window.removeEventListener("beforeunload", prevent);
    };
  }, []);
  function change(definition) {
    dirty.current = true;
    setState({ ...state, definition });
  }
  function field(index, changes) {
    change({
      ...state.definition,
      fields: state.definition.fields.map((item, i) =>
        i === index ? { ...item, ...changes } : item,
      ),
    });
  }
  async function save(publish) {
    if (busy) return;
    setBusy(true);
    setNotice("");
    try {
      const result = await request(endpoint, {
        csrf: host.dataset.csrf,
        version: state.version,
        definition: state.definition,
        publish,
      });
      setState((previous) => {
        if (previous.definition === state.definition) dirty.current = false;
        return {
          ...previous,
          version: result.version,
          published_version: publish
            ? result.version
            : previous.published_version,
        };
      });
      setError(false);
      setNotice(
        publish
          ? "Published. Visitors will now see this version."
          : "Working copy saved. The published version is unchanged.",
      );
    } catch (err) {
      setError(true);
      setNotice(err.message);
    } finally {
      setBusy(false);
    }
  }
  if (!state)
    return (
      <>
        {notice ? (
          <Notice error>{notice}</Notice>
        ) : (
          <p>Loading local form designer…</p>
        )}
      </>
    );
  const definition = state.definition;
  return (
    <>
      <div class="toolbar">
        <Button busy={busy} onClick={() => save(false)}>
          Save working copy
        </Button>
        <Button variant="secondary" busy={busy} onClick={() => save(true)}>
          Publish form
        </Button>
        {state.published_version > 0 && (
          <a href={`/forms/${host.dataset.id}`}>Open published form ↗</a>
        )}
      </div>
      {notice && <Notice error={error}>{notice}</Notice>}
      <section class="panel">
        <Field
          label="Form title"
          value={definition.title}
          maxLength={160}
          onInput={(event) =>
            change({ ...definition, title: event.currentTarget.value })
          }
        />
        <Field
          label="Maximum accepted responses"
          type="number"
          min="1"
          max="1000000"
          value={definition.max_entries}
          onInput={(event) =>
            change({
              ...definition,
              max_entries: Number(event.currentTarget.value),
            })
          }
          description="The server stops accepting new responses at this limit. Retries of an accepted response remain safe."
        />
      </section>
      {definition.fields.map((item, index) => (
        <section class="panel" key={item.name}>
          <div class="toolbar">
            <h2>{item.schema.label || `Field ${index + 1}`}</h2>
            <Button
              variant="quiet"
              disabled={index === 0 || busy}
              aria-label={`Move ${item.schema.label || item.name} up`}
              onClick={() => {
                const fields = [...definition.fields];
                [fields[index - 1], fields[index]] = [
                  fields[index],
                  fields[index - 1],
                ];
                change({ ...definition, fields });
              }}
            >
              Move up
            </Button>
            <Button
              variant="quiet"
              disabled={definition.fields.length === 1 || busy}
              onClick={() =>
                change({
                  ...definition,
                  fields: definition.fields.filter((_, i) => i !== index),
                })
              }
            >
              Remove field
            </Button>
          </div>
          <div class="field-row">
            <Field
              label="Label"
              value={item.schema.label}
              maxLength={100}
              onInput={(event) =>
                field(index, {
                  schema: { ...item.schema, label: event.currentTarget.value },
                })
              }
            />
            <Select
              label="Input"
              value={item.schema.kind}
              disabled={
                !["string", "number", "boolean"].includes(item.schema.kind)
              }
              onChange={(event) =>
                field(index, {
                  schema: { ...item.schema, kind: event.currentTarget.value },
                  calculation: null,
                })
              }
            >
              <option value="string">Text</option>
              <option value="number">Number</option>
              <option value="boolean">Checkbox</option>
              {!["string", "number", "boolean"].includes(item.schema.kind) && (
                <option value={item.schema.kind}>Structured input</option>
              )}
            </Select>
            <Field
              label="Step"
              type="number"
              min="1"
              max="8"
              value={(item.step || 0) + 1}
              onInput={(event) =>
                field(index, { step: Number(event.currentTarget.value) - 1 })
              }
            />
          </div>
          <label>
            <input
              type="checkbox"
              checked={item.schema.required}
              onChange={(event) =>
                field(index, {
                  schema: {
                    ...item.schema,
                    required: event.currentTarget.checked,
                  },
                })
              }
            />{" "}
            Required
          </label>
          <Disclosure summary="Visibility and calculation">
            <Select
              label="Show when a previous field has a value"
              value={item.visible_when?.field || ""}
              onChange={(event) =>
                field(index, {
                  visible_when: event.currentTarget.value
                    ? { operation: "Present", field: event.currentTarget.value }
                    : null,
                })
              }
            >
              <option value="">Always visible</option>
              {definition.fields.slice(0, index).map((previous) => (
                <option value={previous.name}>
                  {previous.schema.label || previous.name}
                </option>
              ))}
            </Select>
            {item.visible_when && (
              <Select
                label="Condition"
                value={item.visible_when.operation}
                onChange={(event) =>
                  field(index, {
                    visible_when:
                      event.currentTarget.value === "Equal"
                        ? {
                            operation: "Equal",
                            field: item.visible_when.field,
                            value: true,
                          }
                        : {
                            operation: "Present",
                            field: item.visible_when.field,
                          },
                  })
                }
              >
                <option value="Present">Has a value</option>
                <option value="Equal">Equals a value</option>
              </Select>
            )}
            {item.visible_when?.operation === "Equal" && (
              <Field
                label="Value to match"
                value={String(item.visible_when.value)}
                onInput={(event) => {
                  const kind = definition.fields.find(
                    (previous) => previous.name === item.visible_when.field,
                  )?.schema.kind;
                  const raw = event.currentTarget.value;
                  field(index, {
                    visible_when: {
                      ...item.visible_when,
                      value:
                        kind === "boolean"
                          ? raw === "true"
                          : kind === "number"
                            ? Number(raw)
                            : raw,
                    },
                  });
                }}
              />
            )}
            {item.schema.kind === "number" && (
              <>
                <Select
                  label="Calculated value"
                  value={item.calculation?.operation || ""}
                  onChange={(event) =>
                    field(index, {
                      calculation: event.currentTarget.value
                        ? { operation: event.currentTarget.value, fields: [] }
                        : null,
                    })
                  }
                >
                  <option value="">Visitor enters a number</option>
                  <option value="Sum">Sum</option>
                  <option value="Product">Product</option>
                </Select>
                {item.calculation && (
                  <fieldset>
                    <legend>Earlier numbers to calculate</legend>
                    {definition.fields
                      .slice(0, index)
                      .filter((previous) => previous.schema.kind === "number")
                      .map((previous) => (
                        <label>
                          <input
                            type="checkbox"
                            checked={item.calculation.fields.includes(
                              previous.name,
                            )}
                            onChange={(event) =>
                              field(index, {
                                calculation: {
                                  ...item.calculation,
                                  fields: event.currentTarget.checked
                                    ? [
                                        ...item.calculation.fields,
                                        previous.name,
                                      ]
                                    : item.calculation.fields.filter(
                                        (name) => name !== previous.name,
                                      ),
                                },
                              })
                            }
                          />{" "}
                          {previous.schema.label || previous.name}
                        </label>
                      ))}
                  </fieldset>
                )}
              </>
            )}
          </Disclosure>
        </section>
      ))}
      <Button
        variant="secondary"
        disabled={definition.fields.length >= 32 || busy}
        onClick={() =>
          change({
            ...definition,
            fields: [
              ...definition.fields,
              {
                name: `field_${crypto.randomUUID().replaceAll("-", "").slice(0, 12)}`,
                schema: { kind: "string", label: "New field", required: false },
                step: definition.fields.at(-1)?.step || 0,
              },
            ],
          })
        }
      >
        Add field
      </Button>
    </>
  );
}
function evaluated(fields, input) {
  const values = {};
  const visible = [];
  for (const field of fields) {
    const condition = field.visible_when;
    const shown =
      !condition ||
      (condition.operation === "Equal"
        ? values[condition.field] === condition.value
        : values[condition.field] !== undefined &&
          values[condition.field] !== null &&
          String(values[condition.field]).trim() !== "");
    if (!shown) continue;
    visible.push(field);
    if (field.calculation) {
      const operands = field.calculation.fields.map((name) => values[name]);
      if (
        operands.every(
          (value) => typeof value === "number" && Number.isFinite(value),
        )
      ) {
        const value =
          field.calculation.operation === "Sum"
            ? operands.reduce((a, b) => a + b, 0)
            : operands.reduce((a, b) => a * b, 1);
        if (Number.isFinite(value)) values[field.name] = value;
      }
    } else if (Object.hasOwn(input, field.name))
      values[field.name] = input[field.name];
  }
  return { values, visible };
}
function VisitorForm({ host }) {
  const published = JSON.parse(host.dataset.definition);
  const definition = published.form;
  const [values, setValues] = useState(() =>
    Object.fromEntries(
      definition.fields
        .filter((field) => field.schema.kind === "boolean")
        .map((field) => [field.name, false]),
    ),
  );
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [error, setError] = useState("");
  const key = useRef(crypto.randomUUID());
  const current = evaluated(definition.fields, values);
  const last = Math.max(...definition.fields.map((field) => field.step || 0));
  async function submit(event) {
    event.preventDefault();
    if (busy) return;
    if (step < last) {
      setStep(step + 1);
      return;
    }
    setBusy(true);
    setError("");
    try {
      await request(`/api/forms/${host.dataset.id}/entries`, {
        key: key.current,
        version: Number(host.dataset.version),
        values: current.values,
      });
      setAccepted(true);
    } catch (err) {
      setError(err.message);
    } finally {
      setBusy(false);
    }
  }
  if (accepted)
    return <Notice>Your response has been received. Thank you.</Notice>;
  return (
    <form onSubmit={submit}>
      {last > 0 && (
        <p>
          Step {step + 1} of {last + 1}
        </p>
      )}
      {error && <Notice error>{error}</Notice>}
      {current.visible
        .filter((field) => (field.step || 0) === step)
        .map((field) => {
          const value = current.values[field.name];
          const update = (value) =>
            setValues({ ...values, [field.name]: value });
          if (field.calculation)
            return (
              <p>
                <strong>{field.schema.label || field.name}: </strong>
                <output>
                  {value === undefined ? "Complete the earlier numbers" : value}
                </output>
              </p>
            );
          if (field.schema.kind === "boolean")
            return (
              <label>
                <input
                  type="checkbox"
                  checked={value === true}
                  onChange={(event) => update(event.currentTarget.checked)}
                />
                {field.schema.label || field.name}
              </label>
            );
          if (field.schema.kind === "string" || field.schema.kind === "number")
            return (
              <Field
                label={field.schema.label || field.name}
                required={field.schema.required}
                type={field.schema.kind === "number" ? "number" : "text"}
                step={field.schema.kind === "number" ? "any" : undefined}
                maxLength={8000}
                value={value ?? ""}
                onInput={(event) => {
                  const raw = event.currentTarget.value;
                  if (field.schema.kind === "number") {
                    const copy = { ...values };
                    if (raw === "") delete copy[field.name];
                    else copy[field.name] = Number(raw);
                    setValues(copy);
                  } else update(raw);
                }}
              />
            );
          return (
            <Notice error>
              This structured input is not yet available in the form interface.
            </Notice>
          );
        })}
      <div class="toolbar">
        {step > 0 && (
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => setStep(step - 1)}
          >
            Back
          </Button>
        )}
        <Button type="submit" busy={busy}>
          {step < last ? "Continue" : "Send response"}
        </Button>
      </div>
    </form>
  );
}
const studio = document.querySelector("#forms-studio");
if (studio) {
  studio.replaceChildren();
  render(<Designer host={studio} />, studio);
}
const publicForm = document.querySelector("#public-form");
if (publicForm) {
  publicForm.replaceChildren();
  render(<VisitorForm host={publicForm} />, publicForm);
}
