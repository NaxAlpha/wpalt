import { h, Fragment } from "preact";
import { useState, useRef, useEffect } from "preact/hooks";
import { Button, Field, SelectField, Disclosure, Notice } from "./ui.jsx";

const copy = (value) => JSON.parse(JSON.stringify(value));
const safe = (name) =>
  /^[a-z][a-z0-9_-]{0,63}$/.test(name) &&
  !["constructor", "prototype", "__proto__"].includes(name);
const field = (kind, label = "") => ({ kind, label });
const bind = (name, datatype = "Text") => ({
  kind: "field",
  path: [name],
  datatype,
});
const publication = (attribute) => ({ kind: "publication", attribute });
const entity = (types, properties, scope = []) => ({
  types: [types],
  properties,
  scope,
});

// These helpers create ordinary optional typed fields. They do not invent facts,
// accounts, dates, eligibility or a parallel location database.
function preset(kind) {
  const fields = {};
  let entities;
  if (kind === "article") {
    entities = {
      article: entity("Article", {
        headline: publication("title"),
        datePublished: publication("published_at"),
        mainEntityOfPage: publication("url"),
        inLanguage: publication("language"),
      }),
    };
  } else if (kind === "breadcrumb") {
    fields.breadcrumbs = {
      kind: "repeater",
      label: "Breadcrumbs",
      max_items: 10,
      fields: {
        position: { kind: "integer", label: "Position", min: 1, max: 10 },
        name: field("string", "Page name"),
        url: field("url", "Page URL"),
      },
    };
    entities = {
      breadcrumb: entity("BreadcrumbList", {
        itemListElement: { kind: "entity", entity: "crumb" },
      }),
      crumb: entity(
        "ListItem",
        {
          position: bind("position", "Integer"),
          name: bind("name"),
          item: bind("url", "URL"),
        },
        ["breadcrumbs"],
      ),
    };
  } else {
    fields.public_name = field("string", "Public name");
    const type =
      kind === "person"
        ? "Person"
        : kind === "organization"
          ? "Organization"
          : "LocalBusiness";
    entities = {
      [kind]: entity(type, {
        name: bind("public_name"),
        url: publication("url"),
      }),
    };
    if (kind === "location") {
      fields.telephone = field("string", "Public telephone");
      fields.address = {
        kind: "object",
        label: "Address",
        fields: {
          street: field("string", "Street"),
          city: field("string", "City"),
          postal_code: field("string", "Postal code"),
          country: field("string", "Country"),
        },
      };
      fields.coordinates = {
        kind: "object",
        label: "Coordinates",
        fields: {
          latitude: { kind: "number", label: "Latitude", min: -90, max: 90 },
          longitude: {
            kind: "number",
            label: "Longitude",
            min: -180,
            max: 180,
          },
        },
      };
      const days = Object.fromEntries(
        [
          "Monday",
          "Tuesday",
          "Wednesday",
          "Thursday",
          "Friday",
          "Saturday",
          "Sunday",
        ].map((d) => [d.toLowerCase(), d]),
      );
      fields.opening_hours = {
        kind: "repeater",
        label: "Opening hours",
        max_items: 7,
        fields: {
          days: { kind: "choices", label: "Days", choices: days, max_items: 7 },
          opens: field("string", "Opens (HH:MM)"),
          closes: field("string", "Closes (HH:MM)"),
        },
      };
      Object.assign(entities.location.properties, {
        telephone: bind("telephone"),
        address: { kind: "entity", entity: "address" },
        geo: { kind: "entity", entity: "coordinates" },
        openingHoursSpecification: { kind: "entity", entity: "hours" },
      });
      entities.address = entity(
        "PostalAddress",
        {
          streetAddress: bind("street"),
          addressLocality: bind("city"),
          postalCode: bind("postal_code"),
          addressCountry: bind("country"),
        },
        ["address"],
      );
      entities.coordinates = entity(
        "GeoCoordinates",
        {
          latitude: bind("latitude", "Number"),
          longitude: bind("longitude", "Number"),
        },
        ["coordinates"],
      );
      entities.hours = entity(
        "OpeningHoursSpecification",
        {
          dayOfWeek: {
            kind: "enumeration",
            path: ["days"],
            choices: Object.fromEntries(
              Object.keys(days).map((k) => [k, days[k]]),
            ),
          },
          opens: bind("opens", "Time"),
          closes: bind("closes", "Time"),
        },
        ["opening_hours"],
      );
    }
  }
  return {
    fields,
    structured: {
      vocabulary: "30.1",
      primary: Object.keys(entities)[0],
      allow_pending: false,
      entities,
    },
  };
}

function TermSearch({ label, family, pending, onSelect }) {
  const [prefix, setPrefix] = useState("");
  const [page, setPage] = useState(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [scope, setScope] = useState("");
  const active = useRef(null);
  useEffect(() => {
    active.current?.abort();
    setPage(null);
    setError("");
    setBusy(false);
    return () => active.current?.abort();
  }, [family, pending]);
  async function search(after = "") {
    active.current?.abort();
    const controller = new AbortController();
    active.current = controller;
    setBusy(true);
    setError("");
    const key = `${family}:${pending}:${prefix}`;
    try {
      const query = new URLSearchParams({
        family,
        prefix,
        pending: String(pending),
        after,
      });
      const response = await fetch(
        `/api/admin/structured/vocabulary?${query}`,
        {
          credentials: "same-origin",
          signal: controller.signal,
          headers: { Accept: "application/json" },
        },
      );
      if (!response.ok)
        throw new Error(
          "Term lookup failed. Check your owner session and search prefix.",
        );
      const next = await response.json();
      if (!controller.signal.aborted) {
        setPage(next);
        setScope(key);
      }
    } catch (e) {
      if (!controller.signal.aborted) {
        setError(e.message);
        setPage(null);
      }
    } finally {
      if (active.current === controller) setBusy(false);
    }
  }
  return (
    <Disclosure summary={label}>
      <Field
        label={`${label} prefix`}
        value={prefix}
        maxlength="64"
        description="Known term names; spaces are ignored. Search stays on this server."
        onInput={(e) => {
          active.current?.abort();
          setBusy(false);
          setPrefix(e.currentTarget.value);
          setPage(null);
        }}
      />
      <Button variant="secondary" busy={busy} onClick={() => search()}>
        Search {family} terms
      </Button>
      {error && <Notice error>{error}</Notice>}
      {page && (
        <>
          {page.items.length === 0 && <p>No matching active terms.</p>}
          <div class="structured-term-results">
            {page.items.map((term) => (
              <Button
                key={term.name}
                variant="quiet"
                onClick={() => onSelect(term.name)}
              >
                {term.name}
                {term.pending ? " · pending" : ""}
              </Button>
            ))}
          </div>
          {page.after && scope === `${family}:${pending}:${prefix}` && (
            <Button
              variant="secondary"
              busy={busy}
              onClick={() => search(page.after)}
            >
              Next term page
            </Button>
          )}
        </>
      )}
    </Disclosure>
  );
}
function children(field, common) {
  return field.group ? common.groups[field.group] || {} : field.fields || {};
}
function paths(fields, common, kinds, prefix = [], depth = 0) {
  if (depth >= 8) return [];
  return Object.entries(fields).flatMap(([name, f]) => {
    const path = [...prefix, name];
    return [
      ...(kinds.includes(f.kind) ? [{ path, field: f }] : []),
      ...(["object", "group"].includes(f.kind)
        ? paths(children(f, common), common, kinds, path, depth + 1)
        : []),
    ];
  });
}
function selectedField(fields, common, path) {
  let found;
  for (const part of path || []) {
    found = fields[part];
    fields = found ? children(found, common) : {};
  }
  return found;
}
const scalarKinds = [
  "string",
  "email",
  "url",
  "date",
  "choice",
  "choices",
  "integer",
  "number",
  "boolean",
];
function BindingControl({
  name,
  value,
  fields,
  common,
  graph,
  models,
  pending,
  onChange,
}) {
  const pathsFor = (kind) =>
    paths(
      fields,
      common,
      kind === "relationship"
        ? ["relationship", "relationships"]
        : kind === "image"
          ? ["media", "gallery"]
          : kind === "enumeration"
            ? ["choice", "choices"]
            : scalarKinds,
    );
  const setKind = (kind) => {
    if (kind === "publication") onChange(publication("title"));
    else if (kind === "entity") onChange({ kind, entity: graph.primary });
    else if (kind === "constant_enumeration") onChange({ kind, value: "" });
    else {
      const path = pathsFor(kind)[0]?.path || [];
      onChange(
        kind === "field"
          ? { kind, path, datatype: "Text" }
          : kind === "enumeration"
            ? { kind, path, choices: {} }
            : kind === "relationship"
              ? { kind, path, entity: "" }
              : { kind, path },
      );
    }
  };
  const f = selectedField(fields, common, value.path);
  return (
    <>
      <SelectField
        label={`${name} source`}
        value={value.kind}
        onChange={(e) => setKind(e.currentTarget.value)}
      >
        <option value="field">Typed field</option>
        <option value="publication">Publication fact</option>
        <option value="entity">Named entity on this record</option>
        <option value="relationship">Published related entity</option>
        <option value="image">Local admitted image</option>
        <option value="enumeration">Map typed choices</option>
        <option value="constant_enumeration">Explicit enumeration</option>
      </SelectField>
      {["field", "enumeration", "relationship", "image"].includes(
        value.kind,
      ) && (
        <SelectField
          label={`${name} field`}
          value={(value.path || []).join(".")}
          onChange={(e) =>
            onChange({
              ...value,
              path: e.currentTarget.value.split("."),
              ...(value.kind === "enumeration"
                ? { choices: {} }
                : value.kind === "relationship"
                  ? { entity: "" }
                  : {}),
            })
          }
        >
          <option value="">Choose a declared compatible field</option>
          {pathsFor(value.kind).map(({ path, field }) => (
            <option value={path.join(".")}>
              {field.label || path.join(".")} · {field.kind}
            </option>
          ))}
        </SelectField>
      )}
      {value.kind === "field" && (
        <SelectField
          label={`${name} datatype`}
          value={value.datatype}
          description="Text and URL are explicit Schema.org fallbacks. Other datatypes must match both field semantics and the property range."
          onChange={(e) =>
            onChange({ ...value, datatype: e.currentTarget.value })
          }
        >
          {[
            "Text",
            "URL",
            "Date",
            "DateTime",
            "Time",
            "Integer",
            "Number",
            "Boolean",
          ].map((t) => (
            <option value={t}>{t}</option>
          ))}
        </SelectField>
      )}
      {value.kind === "publication" && (
        <SelectField
          label={`${name} publication fact`}
          value={value.attribute}
          onChange={(e) => onChange(publication(e.currentTarget.value))}
        >
          <option value="title">Published content title</option>
          <option value="url">Canonical content URL</option>
          <option value="published_at">Original publication date</option>
          <option value="language">Published language</option>
        </SelectField>
      )}
      {["entity", "relationship"].includes(value.kind) && (
        <SelectField
          label={`${name} target entity`}
          value={value.entity}
          description={
            value.kind === "relationship"
              ? "Save the target model's declaration first. Private and unpublished records are omitted."
              : "Named cycles use finite identities. A repeater target emits each typed item."
          }
          onChange={(e) =>
            onChange({ ...value, entity: e.currentTarget.value })
          }
        >
          <option value="">Choose an entity</option>
          {Object.keys(
            value.kind === "entity"
              ? graph.entities
              : models[f?.target]?.structured?.entities || {},
          ).map((e) => (
            <option value={e}>{e}</option>
          ))}
        </SelectField>
      )}
      {value.kind === "enumeration" &&
        Object.entries(f?.choices || {}).map(([key, label]) => (
          <div key={key} class="structured-choice-map">
            <p>
              {label}: {value.choices[key] || "Choose an enumeration value"}
            </p>
            <TermSearch
              label={`${name} enumeration for ${label}`}
              family="enumeration"
              pending={pending}
              onSelect={(term) =>
                onChange({
                  ...value,
                  choices: { ...value.choices, [key]: term },
                })
              }
            />
          </div>
        ))}
      {value.kind === "constant_enumeration" && (
        <>
          <p>Explicit value: {value.value || "Choose a value"}</p>
          <TermSearch
            label={`${name} enumeration`}
            family="enumeration"
            pending={pending}
            onSelect={(term) => onChange({ ...value, value: term })}
          />
        </>
      )}
    </>
  );
}

export function StructuredModel({ model, common, models, onChange }) {
  const [start, setStart] = useState("article");
  const [newName, setNewName] = useState("");
  const [error, setError] = useState("");
  const graph = model.structured;
  const allFields = { ...common.fields, ...model.fields };
  function update(fn) {
    const next = copy(model);
    fn(next.structured);
    onChange(next);
  }
  function create() {
    const next = preset(start);
    const collisions = Object.keys(next.fields).filter((k) =>
      Object.hasOwn(allFields, k),
    );
    if (collisions.length) {
      setError(
        `Helper fields already exist: ${collisions.join(", ")}. Use a custom declaration to bind existing fields.`,
      );
      return;
    }
    setError("");
    onChange({
      ...model,
      fields: { ...model.fields, ...next.fields },
      structured: next.structured,
    });
  }
  return (
    <Disclosure summary="Published structured facts" initialOpen={!!graph}>
      <p>
        Use the same typed content fields for visible facts and JSON-LD. Save
        this model to validate existing working and published data. Model
        changes apply immediately; content values change when published. No
        account names or draft relationships are inferred.
      </p>
      <p class="help">
        Schema.org 30.1 · dated selected Google profiles, reviewed 11 October
        2026. Vocabulary validity does not promise indexing or rich results.
        Google FAQ rich results retired in May 2026.
      </p>
      {error && <Notice error>{error}</Notice>}
      {!graph ? (
        <>
          <SelectField
            label="Structured starting definition"
            value={start}
            onChange={(e) => setStart(e.currentTarget.value)}
          >
            <option value="article">Article</option>
            <option value="person">Public person</option>
            <option value="organization">Organization</option>
            <option value="location">
              Business location, address and hours
            </option>
            <option value="breadcrumb">Breadcrumbs</option>
          </SelectField>
          <Button variant="secondary" onClick={create}>
            Create structured definition
          </Button>
        </>
      ) : (
        <>
          <Field
            label="Explicitly admit pending vocabulary terms"
            type="checkbox"
            checked={!!graph.allow_pending}
            onChange={(e) =>
              update((g) => {
                g.allow_pending = e.currentTarget.checked;
              })
            }
          />
          <SelectField
            label="Primary entity"
            value={graph.primary}
            onChange={(e) =>
              update((g) => {
                g.primary = e.currentTarget.value;
              })
            }
          >
            {Object.keys(graph.entities)
              .filter((key) => !graph.entities[key].scope?.length)
              .map((key) => (
                <option value={key}>{key}</option>
              ))}
          </SelectField>
          {Object.entries(graph.entities).map(([name, e]) => {
            const scopeField = selectedField(allFields, common, e.scope);
            const fields = e.scope?.length
              ? children(scopeField || {}, common)
              : allFields;
            return (
              <Disclosure
                key={name}
                summary={`Entity ${name} · ${e.types.join(", ")}`}
                initialOpen={name === graph.primary}
              >
                <SelectField
                  label={`${name} field scope`}
                  value={(e.scope || []).join(".")}
                  disabled={name === graph.primary}
                  onChange={(event) =>
                    update((g) => {
                      g.entities[name].scope = event.currentTarget.value
                        ? event.currentTarget.value.split(".")
                        : [];
                    })
                  }
                >
                  <option value="">Whole record</option>
                  {paths(allFields, common, [
                    "object",
                    "group",
                    "repeater",
                  ]).map(({ path, field }) => (
                    <option value={path.join(".")}>
                      {field.label || path.join(".")} · {field.kind}
                    </option>
                  ))}
                </SelectField>
                {e.types.map((type) => (
                  <div key={type} class="toolbar">
                    <span>{type}</span>
                    <Button
                      variant="quiet"
                      disabled={e.types.length === 1}
                      onClick={() =>
                        update((g) => {
                          g.entities[name].types = e.types.filter(
                            (t) => t !== type,
                          );
                        })
                      }
                    >
                      Remove {type}
                    </Button>
                  </div>
                ))}
                <TermSearch
                  label={`Add type to ${name}`}
                  family="type"
                  pending={graph.allow_pending}
                  onSelect={(type) =>
                    update((g) => {
                      if (
                        !g.entities[name].types.includes(type) &&
                        g.entities[name].types.length < 4
                      )
                        g.entities[name].types.push(type);
                    })
                  }
                />
                {Object.entries(e.properties).map(([property, binding]) => (
                  <Disclosure key={property} summary={`${name}.${property}`}>
                    <BindingControl
                      name={`${name}.${property}`}
                      value={binding}
                      fields={fields}
                      common={common}
                      graph={graph}
                      models={models}
                      pending={graph.allow_pending}
                      onChange={(binding) =>
                        update((g) => {
                          g.entities[name].properties[property] = binding;
                        })
                      }
                    />
                    <Button
                      variant="quiet"
                      onClick={() =>
                        update((g) => {
                          delete g.entities[name].properties[property];
                        })
                      }
                    >
                      Remove {name}.{property}
                    </Button>
                  </Disclosure>
                ))}
                <TermSearch
                  label={`Add property to ${name}`}
                  family="property"
                  pending={graph.allow_pending}
                  onSelect={(property) =>
                    update((g) => {
                      if (!Object.hasOwn(g.entities[name].properties, property))
                        g.entities[name].properties[property] =
                          publication("title");
                    })
                  }
                />
                {name !== graph.primary && (
                  <Button
                    variant="quiet"
                    onClick={() =>
                      update((g) => {
                        delete g.entities[name];
                      })
                    }
                  >
                    Remove entity {name}
                  </Button>
                )}
              </Disclosure>
            );
          })}
          <Field
            label="New entity identifier"
            value={newName}
            maxlength="64"
            description="Lowercase name; up to sixteen entities. Link it from a known property."
            onInput={(e) => setNewName(e.currentTarget.value)}
          />
          <Button
            variant="secondary"
            disabled={
              !safe(newName) ||
              Object.hasOwn(graph.entities, newName) ||
              Object.keys(graph.entities).length >= 16
            }
            onClick={() => {
              update((g) => {
                g.entities[newName] = entity("Thing", {});
              });
              setNewName("");
            }}
          >
            Add named entity
          </Button>
          <Button
            variant="quiet"
            onClick={() => {
              const next = { ...model };
              delete next.structured;
              onChange(next);
            }}
          >
            Remove structured declaration
          </Button>
          <p class="help">
            For multiple locations, create and save a location model first, then
            add a native relationship field to your organization and bind its
            location property to that model's primary entity. Publish separate
            location records. Article authors similarly use a separate public
            person model. Fields stay installed when a declaration is removed.
          </p>
        </>
      )}
    </Disclosure>
  );
}
