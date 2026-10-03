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
  if (!response.ok) {
    const error = new Error(
      result.error ||
        "The request could not be completed. Your work is still here.",
    );
    error.status = response.status;
    throw error;
  }
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
      <section class="panel">
        <h2>Audience subscription</h2>
        <p>
          <a href={`/admin/forms/${host.dataset.id}/workflows`}>
            Conditional notifications and moderated contributions
          </a>
        </p>
        <Select
          label="Subscription list"
          value={definition.subscription?.list || ""}
          onChange={(event) => {
            const list = state.lists.find(
              (list) => list.id === event.currentTarget.value,
            );
            change({
              ...definition,
              subscription: list
                ? {
                    list: list.id,
                    policy: list.policy,
                    email_field: "",
                    consent_field: "",
                  }
                : null,
            });
          }}
        >
          <option value="">Collect responses only</option>
          {state.lists.map((list) => (
            <option value={list.id}>{list.title}</option>
          ))}
        </Select>
        {definition.subscription && (
          <>
            <p>
              {
                state.lists.find(
                  (list) => list.id === definition.subscription.list,
                )?.purpose
              }
            </p>
            <Select
              label="Email address field"
              value={definition.subscription.email_field}
              onChange={(event) =>
                change({
                  ...definition,
                  subscription: {
                    ...definition.subscription,
                    email_field: event.currentTarget.value,
                  },
                })
              }
            >
              <option value="">Choose a text field</option>
              {definition.fields
                .filter((field) => field.schema.kind === "string")
                .map((field) => (
                  <option value={field.name}>
                    {field.schema.label || field.name}
                  </option>
                ))}
            </Select>
            <Select
              label="Explicit consent checkbox"
              value={definition.subscription.consent_field}
              onChange={(event) =>
                change({
                  ...definition,
                  subscription: {
                    ...definition.subscription,
                    consent_field: event.currentTarget.value,
                  },
                })
              }
            >
              <option value="">Choose a checkbox</option>
              {definition.fields
                .filter((field) => field.schema.kind === "boolean")
                .map((field) => (
                  <option value={field.name}>
                    {field.schema.label || field.name}
                  </option>
                ))}
            </Select>
            <p>
              Leave consent unchecked by default. Address confirmation is
              required before campaign delivery.
            </p>
          </>
        )}
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
                !["string", "number", "boolean", "object", "repeater"].includes(
                  item.schema.kind,
                )
              }
              onChange={(event) =>
                field(index, {
                  schema: { ...item.schema, kind: event.currentTarget.value },
                  calculation: null,
                  widget: null,
                })
              }
            >
              <option value="string">Text</option>
              <option value="number">Number</option>
              <option value="boolean">Checkbox</option>
              <option value="object">Group</option>
              <option value="repeater">Repeatable group</option>
              {!["string", "number", "boolean", "object", "repeater"].includes(
                item.schema.kind,
              ) && <option value={item.schema.kind}>Structured input</option>}
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
          {item.widget?.kind === "Signature" && (
            <Field
              label="Signature statement"
              multiline
              value={item.widget.statement}
              maxLength={4000}
              onInput={(event) =>
                field(index, {
                  widget: {
                    ...item.widget,
                    statement: event.currentTarget.value,
                  },
                })
              }
            />
          )}
          {["object", "repeater"].includes(item.schema.kind) && (
            <>
              {item.schema.kind === "repeater" && (
                <Field
                  label="Maximum repeatable rows"
                  type="number"
                  min="1"
                  max="50"
                  value={item.schema.max_items || 20}
                  onInput={(event) =>
                    field(index, {
                      schema: {
                        ...item.schema,
                        max_items: Number(event.currentTarget.value),
                      },
                    })
                  }
                />
              )}
              <FieldsDesigner
                fields={item.schema.fields}
                change={(fields) =>
                  field(index, { schema: { ...item.schema, fields } })
                }
              />
            </>
          )}
          {["string", "boolean"].includes(item.schema.kind) && (
            <Select
              label="Presentation"
              value={item.widget?.kind || ""}
              onChange={(event) =>
                field(index, {
                  widget: event.currentTarget.value
                    ? {
                        kind: event.currentTarget.value,
                        ...(event.currentTarget.value === "Choice"
                          ? {
                              options: [
                                { label: "Option", value: "option", score: 0 },
                              ],
                            }
                          : event.currentTarget.value === "Acknowledgment"
                            ? { statement: "I accept this statement" }
                            : {}),
                      }
                    : null,
                })
              }
            >
              <option value="">Default input</option>
              {item.schema.kind === "string" && (
                <>
                  <option value="Email">Email address</option>
                  <option value="Upload">Protected attachment</option>
                  <option value="TextArea">Long text</option>
                  <option value="Choice">Survey / poll choice</option>
                </>
              )}
              {item.schema.kind === "boolean" && (
                <option value="Acknowledgment">Acknowledgment</option>
              )}
            </Select>
          )}
          {item.widget?.kind === "Acknowledgment" && (
            <Field
              label="Acknowledgment statement"
              multiline
              value={item.widget.statement}
              maxLength={4000}
              onInput={(event) =>
                field(index, {
                  widget: {
                    ...item.widget,
                    statement: event.currentTarget.value,
                  },
                })
              }
            />
          )}
          {item.widget?.kind === "Choice" && (
            <fieldset>
              <legend>Published choices and scores</legend>
              {item.widget.options.map((option, choice) => (
                <div class="field-row">
                  <Field
                    label="Choice label"
                    value={option.label}
                    maxLength={160}
                    onInput={(event) =>
                      field(index, {
                        widget: {
                          ...item.widget,
                          options: item.widget.options.map((o, i) =>
                            i === choice
                              ? { ...o, label: event.currentTarget.value }
                              : o,
                          ),
                        },
                      })
                    }
                  />
                  <Field
                    label="Choice score"
                    type="number"
                    min="-10000"
                    max="10000"
                    value={option.score}
                    onInput={(event) =>
                      field(index, {
                        widget: {
                          ...item.widget,
                          options: item.widget.options.map((o, i) =>
                            i === choice
                              ? {
                                  ...o,
                                  score: Number(event.currentTarget.value),
                                }
                              : o,
                          ),
                        },
                      })
                    }
                  />
                  <Button
                    variant="quiet"
                    disabled={item.widget.options.length === 1}
                    onClick={() =>
                      field(index, {
                        widget: {
                          ...item.widget,
                          options: item.widget.options.filter(
                            (_, i) => i !== choice,
                          ),
                        },
                      })
                    }
                  >
                    Remove choice
                  </Button>
                </div>
              ))}
              <Button
                variant="secondary"
                disabled={item.widget.options.length >= 32}
                onClick={() =>
                  field(index, {
                    widget: {
                      ...item.widget,
                      options: [
                        ...item.widget.options,
                        {
                          label: "New option",
                          value: crypto.randomUUID(),
                          score: 0,
                        },
                      ],
                    },
                  })
                }
              >
                Add choice
              </Button>
            </fieldset>
          )}
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
                  <option value="Score">Survey score</option>
                </Select>
                {item.calculation && (
                  <fieldset>
                    <legend>Earlier numbers to calculate</legend>
                    {definition.fields
                      .slice(0, index)
                      .filter((previous) =>
                        item.calculation.operation === "Score"
                          ? previous.widget?.kind === "Choice"
                          : previous.schema.kind === "number",
                      )
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
                name: `signature_${crypto.randomUUID().replaceAll("-", "").slice(0, 12)}`,
                schema: {
                  kind: "object",
                  label: "Signature",
                  required: true,
                  fields: {
                    name: {
                      kind: "string",
                      label: "Your name",
                      required: true,
                    },
                    accepted: {
                      kind: "boolean",
                      label: "Acceptance",
                      required: true,
                    },
                  },
                },
                widget: {
                  kind: "Signature",
                  statement: "I confirm that this response is accurate.",
                },
                step: definition.fields.at(-1)?.step || 0,
              },
            ],
          })
        }
      >
        Add signature acknowledgment
      </Button>
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
      const operands = field.calculation.fields.map((name) =>
        field.calculation.operation === "Score"
          ? fields
              .find((f) => f.name === name)
              ?.widget?.options?.find((o) => o.value === values[name])?.score
          : values[name],
      );
      if (
        operands.every(
          (value) => typeof value === "number" && Number.isFinite(value),
        )
      ) {
        const value = ["Sum", "Score"].includes(field.calculation.operation)
          ? operands.reduce((a, b) => a + b, 0)
          : operands.reduce((a, b) => a * b, 1);
        if (Number.isFinite(value)) values[field.name] = value;
      }
    } else if (Object.hasOwn(input, field.name))
      values[field.name] = input[field.name];
  }
  return { values, visible };
}
function ValueInput({
  schema,
  widget,
  value,
  update,
  groups = {},
  name,
  formId,
  version,
  uploadActivity,
}) {
  const label = schema.label || name;
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState("");
  if (widget?.kind === "Upload")
    return (
      <>
        <Field
          label={label}
          type="file"
          accept=".txt,.pdf,.png,.jpg,.jpeg,.webp"
          disabled={uploading}
          required={schema.required && !value}
          onChange={async (event) => {
            const file = event.currentTarget.files?.[0];
            if (!file) return;
            if (file.size > 2 * 1024 * 1024) {
              setUploadError("Choose a file up to 2 MiB.");
              return;
            }
            setUploading(true);
            uploadActivity?.(1);
            setUploadError("");
            try {
              const response = await fetch(
                `/api/forms/${formId}/attachments/${name}`,
                {
                  method: "POST",
                  headers: {
                    "Content-Type": file.type || "application/octet-stream",
                    "x-form-version": String(version),
                    "x-file-name": encodeURIComponent(file.name),
                  },
                  body: file,
                  credentials: "same-origin",
                },
              );
              const result = await response.json().catch(() => ({
                error:
                  "Upload failed. The request or configured file limit may have been exceeded.",
              }));
              if (!response.ok) throw new Error(result.error);
              update(result.capability);
            } catch (error) {
              setUploadError(error.message);
            } finally {
              setUploading(false);
              uploadActivity?.(-1);
            }
          }}
        />
        {uploading && <p role="status">Uploading private attachment…</p>}
        {value && (
          <p>Private attachment ready. Send your response within 24 hours.</p>
        )}
        {uploadError && <Notice error>{uploadError}</Notice>}
      </>
    );

  if (widget?.kind === "Choice")
    return (
      <Select
        label={label}
        required={schema.required}
        value={value ?? ""}
        onChange={(event) => update(event.currentTarget.value || undefined)}
      >
        <option value="">Choose an option</option>
        {widget.options.map((option) => (
          <option value={option.value}>{option.label}</option>
        ))}
      </Select>
    );
  if (widget?.kind === "Signature")
    return (
      <fieldset>
        <legend>{label}</legend>
        <p>{widget.statement}</p>
        <Field
          label={`${label}: your name`}
          value={value?.name || ""}
          maxLength={100}
          required={schema.required}
          onInput={(event) =>
            update({
              ...value,
              name: event.currentTarget.value,
              accepted: value?.accepted || false,
            })
          }
        />
        <label>
          <input
            type="checkbox"
            checked={value?.accepted === true}
            required={schema.required}
            onChange={(event) =>
              update({
                ...value,
                name: value?.name || "",
                accepted: event.currentTarget.checked,
              })
            }
          />
          I accept this statement and sign using my name
        </label>
      </fieldset>
    );
  if (schema.kind === "boolean")
    return (
      <label>
        <input
          type="checkbox"
          checked={value === true}
          required={widget?.kind === "Acknowledgment" && schema.required}
          onChange={(event) => update(event.currentTarget.checked)}
        />
        {widget?.statement || label}
      </label>
    );
  if (["string", "number"].includes(schema.kind))
    return (
      <Field
        label={label}
        required={schema.required}
        multiline={widget?.kind === "TextArea"}
        type={
          schema.kind === "number"
            ? "number"
            : widget?.kind === "Email"
              ? "email"
              : "text"
        }
        step={schema.kind === "number" ? "any" : undefined}
        maxLength={8000}
        value={value ?? ""}
        onInput={(event) => {
          const raw = event.currentTarget.value;
          update(
            schema.kind === "number"
              ? raw === ""
                ? undefined
                : Number(raw)
              : raw,
          );
        }}
      />
    );
  const children = schema.group ? groups[schema.group] : schema.fields;
  if (["object", "group"].includes(schema.kind))
    return (
      <fieldset>
        <legend>{label}</legend>
        {Object.entries(children || {}).map(([child, definition]) => (
          <ValueInput
            schema={definition}
            name={`${label}: ${child}`}
            value={value?.[child]}
            groups={groups}
            update={(next) => update({ ...value, [child]: next })}
          />
        ))}
      </fieldset>
    );
  if (schema.kind === "repeater") {
    const rows = Array.isArray(value) ? value : [];
    return (
      <fieldset>
        <legend>{label}</legend>
        {rows.map((row, index) => (
          <fieldset>
            <legend>
              {label} {index + 1}
            </legend>
            {Object.entries(children || {}).map(([child, definition]) => (
              <ValueInput
                schema={definition}
                name={`${label} ${index + 1}: ${child}`}
                value={row[child]}
                groups={groups}
                update={(next) =>
                  update(
                    rows.map((item, i) =>
                      i === index ? { ...item, [child]: next } : item,
                    ),
                  )
                }
              />
            ))}
            <Button
              variant="quiet"
              onClick={() => update(rows.filter((_, i) => i !== index))}
            >
              Remove {label} {index + 1}
            </Button>
          </fieldset>
        ))}
        <Button
          variant="secondary"
          disabled={rows.length >= (schema.max_items || 20)}
          onClick={() => update([...rows, {}])}
        >
          Add {label}
        </Button>
      </fieldset>
    );
  }
  return (
    <Notice error>This input type is unavailable for public collection.</Notice>
  );
}
function FieldsDesigner({ fields, change, depth = 0 }) {
  return (
    <>
      {Object.entries(fields || {}).map(([name, schema]) => (
        <fieldset>
          <legend>{schema.label || name}</legend>
          <Field
            label="Nested field label"
            value={schema.label || ""}
            maxLength={100}
            onInput={(event) =>
              change({
                ...fields,
                [name]: { ...schema, label: event.currentTarget.value },
              })
            }
          />
          <Select
            label="Nested input type"
            value={schema.kind}
            onChange={(event) =>
              change({
                ...fields,
                [name]: {
                  kind: event.currentTarget.value,
                  label: schema.label,
                  required: schema.required,
                  fields: ["object", "repeater"].includes(
                    event.currentTarget.value,
                  )
                    ? {}
                    : undefined,
                },
              })
            }
          >
            <option value="string">Text</option>
            <option value="number">Number</option>
            <option value="boolean">Checkbox</option>
            {depth < 3 && (
              <>
                <option value="object">Group</option>
                <option value="repeater">Repeatable group</option>
              </>
            )}
          </Select>
          <label>
            <input
              type="checkbox"
              checked={schema.required}
              onChange={(event) =>
                change({
                  ...fields,
                  [name]: { ...schema, required: event.currentTarget.checked },
                })
              }
            />
            Required
          </label>
          {["object", "repeater"].includes(schema.kind) && (
            <FieldsDesigner
              depth={depth + 1}
              fields={schema.fields}
              change={(children) =>
                change({ ...fields, [name]: { ...schema, fields: children } })
              }
            />
          )}
          <Button
            variant="quiet"
            onClick={() => {
              const copy = { ...fields };
              delete copy[name];
              change(copy);
            }}
          >
            Remove nested field
          </Button>
        </fieldset>
      ))}
      <Button
        variant="secondary"
        disabled={Object.keys(fields || {}).length >= 32}
        onClick={() =>
          change({
            ...fields,
            [`field_${crypto.randomUUID().replaceAll("-", "").slice(0, 12)}`]: {
              kind: "string",
              label: "Nested field",
              required: false,
            },
          })
        }
      >
        Add nested field
      </Button>
    </>
  );
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
  const [uploads, setUploads] = useState(0);
  const [error, setError] = useState("");
  const key = useRef(crypto.randomUUID());
  const pending = useRef(null);
  const recovery = useRef({ token: null, revision: 0 });
  const [device, setDevice] = useState(false);
  const [recoveryNotice, setRecoveryNotice] = useState("");
  const storageKey = `wpalt:form:${host.dataset.id}`;
  useEffect(() => {
    try {
      const raw = localStorage.getItem(storageKey);
      if (raw && raw.length <= 150000) {
        const saved = JSON.parse(raw);
        if (
          saved.version === Number(host.dataset.version) &&
          saved.expires > Date.now()
        ) {
          setDevice(true);
          setValues(saved.values);
          key.current = saved.key;
          recovery.current = saved.recovery || { token: null, revision: 0 };
          pending.current = saved.pending || null;
          setRecoveryNotice(
            "A device draft is available. Review your values before continuing. An interrupted submission retries the original response.",
          );
        } else localStorage.removeItem(storageKey);
      }
    } catch {
      setRecoveryNotice("The device draft could not be loaded.");
    }
  }, []);
  useEffect(() => {
    if (!device || accepted) return;
    try {
      const record = JSON.stringify({
        version: Number(host.dataset.version),
        expires: Date.now() + 7 * 86400000,
        values,
        key: key.current,
        recovery: recovery.current,
        pending: pending.current,
      });
      if (record.length > 150000) throw new Error("draft too large");
      localStorage.setItem(storageKey, record);
    } catch {
      setRecoveryNotice(
        "This browser could not store the draft. Your current work is still here.",
      );
    }
  }, [device, values, busy, accepted]);
  async function saveDraft() {
    setBusy(true);
    try {
      if (!recovery.current.token) {
        const bytes = new Uint8Array(32);
        crypto.getRandomValues(bytes);
        recovery.current.token = Array.from(bytes, (b) =>
          b.toString(16).padStart(2, "0"),
        ).join("");
      }
      const saved = await request(`/api/forms/${host.dataset.id}/drafts`, {
        token: recovery.current.token,
        revision: recovery.current.revision,
        version: Number(host.dataset.version),
        values: current.values,
      });
      recovery.current.revision = saved.revision;
      setRecoveryNotice(
        `Server draft saved for seven days. Keep this private recovery link: ${location.origin}/forms/${host.dataset.id}#draft=${recovery.current.token}`,
      );
    } catch (error) {
      setRecoveryNotice(error.message);
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    function restore() {
      const token = location.hash.match(/^#draft=([a-f0-9]{64})$/)?.[1];
      if (!token) return;
      history.replaceState(null, "", location.pathname);
      if (pending.current) {
        setRecoveryNotice(
          "Retry the interrupted response before loading another draft.",
        );
        return;
      }
      request(`/api/forms/${host.dataset.id}/drafts/${token}`)
        .then((saved) => {
          if (saved.version !== Number(host.dataset.version))
            throw new Error(
              "The published form has changed. Ask the owner to review the older draft.",
            );
          recovery.current = { token, revision: saved.revision };
          setValues(saved.values);
          setRecoveryNotice("Server draft restored. Review it before sending.");
        })
        .catch((error) => setRecoveryNotice(error.message));
    }
    restore();
    window.addEventListener("hashchange", restore);
    return () => window.removeEventListener("hashchange", restore);
  }, []);
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
      pending.current ||= {
        key: key.current,
        version: Number(host.dataset.version),
        values: current.values,
      };
      await request(`/api/forms/${host.dataset.id}/entries`, pending.current);
      pending.current = null;
      try {
        localStorage.removeItem(storageKey);
      } catch {}
      setAccepted(true);
    } catch (err) {
      if (err.status && err.status < 500) pending.current = null;
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
      {recoveryNotice && <Notice>{recoveryNotice}</Notice>}
      {published.subscription_purpose && (
        <p>
          Optional subscription purpose: {published.subscription_purpose}. You
          will receive an address-confirmation message if you check the consent
          box.
        </p>
      )}
      <Disclosure summary="Save and resume">
        <label>
          <input
            type="checkbox"
            checked={device}
            onChange={(event) => {
              setDevice(event.currentTarget.checked);
              if (!event.currentTarget.checked) {
                try {
                  localStorage.removeItem(storageKey);
                } catch {}
              }
            }}
          />{" "}
          Save this draft on this device for seven days
        </label>
        <p>
          Device drafts contain your response. Use a private device. You can
          keep editing without a connection and retry when online.
        </p>
        <Button variant="secondary" busy={busy} onClick={saveDraft}>
          Save private server draft
        </Button>
      </Disclosure>
      <fieldset disabled={busy || !!pending.current}>
        {current.visible
          .filter((field) => (field.step || 0) === step)
          .map((field) => {
            const value = current.values[field.name];
            const update = (value) =>
              setValues((previous) => ({ ...previous, [field.name]: value }));
            if (field.calculation)
              return (
                <p>
                  <strong>{field.schema.label || field.name}: </strong>
                  <output>
                    {value === undefined
                      ? "Complete the earlier numbers"
                      : value}
                  </output>
                </p>
              );
            return (
              <ValueInput
                schema={field.schema}
                widget={field.widget}
                value={value}
                update={update}
                groups={published.groups}
                name={field.name}
                key={field.name}
                formId={host.dataset.id}
                version={Number(host.dataset.version)}
                uploadActivity={(delta) => setUploads((count) => count + delta)}
              />
            );
          })}
      </fieldset>
      <div class="toolbar">
        {step > 0 && (
          <Button
            variant="secondary"
            disabled={busy || !!pending.current}
            onClick={() => setStep(step - 1)}
          >
            Back
          </Button>
        )}
        <Button type="submit" busy={busy} disabled={uploads > 0}>
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
