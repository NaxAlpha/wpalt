import { h, Fragment } from "preact";
import { useState } from "preact/hooks";
import { Button, Field, SelectField, Disclosure, Notice } from "./ui.jsx";
const copy = (value) => JSON.parse(JSON.stringify(value));
const blank = () => ({
  label: "New link",
  url: "/",
  description: "",
  children: [],
});
function Items({ values, change, depth = 1, prefix }) {
  return (
    <div class="navigation-items">
      {values.map((item, index) => {
        const edit = (key, value) => {
          const next = copy(values);
          next[index][key] = value;
          change(next);
        };
        const move = (offset) => {
          const next = copy(values);
          const target = index + offset;
          [next[index], next[target]] = [next[target], next[index]];
          change(next);
        };
        return (
          <Disclosure
            summary={`${index + 1}. ${item.label || "Untitled item"}`}
            initialOpen={values.length <= 3}
          >
            <Field
              label={`${prefix} item ${index + 1} label`}
              value={item.label}
              maxlength="100"
              onInput={(e) => edit("label", e.currentTarget.value)}
            />
            <Field
              label={`${prefix} item ${index + 1} destination`}
              value={item.url}
              maxlength="2000"
              description="Use a safe site path or HTTPS link. A group with children may leave this empty."
              onInput={(e) => edit("url", e.currentTarget.value)}
            />
            <Field
              label={`${prefix} item ${index + 1} description`}
              value={item.description}
              maxlength="240"
              onInput={(e) => edit("description", e.currentTarget.value)}
            />
            <div class="toolbar">
              <Button
                variant="secondary"
                disabled={index === 0}
                onClick={() => move(-1)}
              >
                Move up
              </Button>
              <Button
                variant="secondary"
                disabled={index + 1 === values.length}
                onClick={() => move(1)}
              >
                Move down
              </Button>
              <Button
                variant="secondary"
                onClick={() => change(values.filter((_, i) => i !== index))}
              >
                Remove item
              </Button>
            </div>
            {item.children.length > 0 && (
              <Items
                values={item.children}
                depth={depth + 1}
                prefix={`${prefix} ${index + 1}`}
                change={(children) => edit("children", children)}
              />
            )}
            {depth < 4 && (
              <Button
                variant="secondary"
                disabled={item.children.length >= 32}
                onClick={() => edit("children", [...item.children, blank()])}
              >
                Add child link
              </Button>
            )}
          </Disclosure>
        );
      })}
      <Button
        variant="secondary"
        disabled={values.length >= 32}
        onClick={() => change([...values, blank()])}
      >
        Add link
      </Button>
    </div>
  );
}
export function NavigationFamilies({ pkg, languages, update }) {
  const [selected, select] = useState("");
  const [name, setName] = useState("");
  const [locale, setLocale] = useState("");
  const [message, setMessage] = useState("");
  const families = pkg.navigations || {};
  const id = families[selected] ? selected : Object.keys(families)[0];
  const family = families[id];
  const active = family?.languages?.[locale] ? locale : "";
  const variant = active ? family.languages[active] : family;
  const edit = (key, value) =>
    update((p) => {
      const f = p.navigations[id];
      (active ? f.languages[active] : f)[key] = value;
    });
  const referenced =
    id &&
    JSON.stringify([
      pkg.header,
      pkg.footer,
      pkg.templates,
      pkg.components,
    ]).includes(`"source":"${id}"`);
  return (
    <Disclosure summary="Theme navigation" collapseOnNarrow>
      <p>
        These menus belong to this theme draft. They become visible after
        publication. Site settings remain available for navigation nodes using
        site links.
      </p>
      {message && <Notice error>{message}</Notice>}
      <Field
        label="New navigation name"
        value={name}
        maxlength="40"
        description="A stable identifier, such as main or footer."
        onInput={(e) => setName(e.currentTarget.value)}
      />
      <Button
        variant="secondary"
        disabled={Object.keys(families).length >= 8 || !languages?.length}
        onClick={() => {
          if (!/^[a-z][a-z0-9_-]{0,39}$/.test(name) || families[name]) {
            setMessage("Choose a unique lowercase navigation identifier.");
            return;
          }
          const language = languages[0];
          update((p) => {
            p.navigations ||= {};
            p.navigations[name] = {
              language: language.code,
              direction: language.direction,
              label: "Website navigation",
              items: [],
              languages: {},
            };
          });
          select(name);
          setName("");
          setLocale("");
          setMessage("");
        }}
      >
        Create navigation
      </Button>
      {family && (
        <>
          <SelectField
            label="Navigation family"
            value={id}
            onChange={(e) => {
              select(e.currentTarget.value);
              setLocale("");
            }}
          >
            {Object.keys(families).map((key) => (
              <option value={key}>{key}</option>
            ))}
          </SelectField>
          <SelectField
            label="Grouped navigation layout"
            value={family.layout || "list"}
            onChange={(e) =>
              update((p) => (p.navigations[id].layout = e.currentTarget.value))
            }
          >
            <option value="list">Hierarchical lists</option>
            <option value="columns">
              Grouped columns · single column on mobile
            </option>
          </SelectField>
          <SelectField
            label="Navigation language variant"
            value={active}
            onChange={(e) => setLocale(e.currentTarget.value)}
          >
            <option value="">Default · {family.language}</option>
            {Object.keys(family.languages).map((code) => (
              <option value={code}>{code}</option>
            ))}
          </SelectField>
          {!active && (
            <SelectField
              label="Default navigation language"
              value={family.language}
              onChange={(e) => {
                const language = languages.find(
                  (l) => l.code === e.currentTarget.value,
                );
                update((p) => {
                  p.navigations[id].language = language.code;
                  p.navigations[id].direction = language.direction;
                });
              }}
            >
              {languages
                .filter((l) => !family.languages[l.code])
                .map((language) => (
                  <option value={language.code}>{language.label}</option>
                ))}
            </SelectField>
          )}
          <Field
            label="Navigation accessible label"
            value={variant.label}
            maxlength="100"
            onInput={(e) => edit("label", e.currentTarget.value)}
          />
          <Items
            values={variant.items}
            change={(items) => edit("items", items)}
            prefix={id}
          />
          <SelectField
            label="Add navigation language"
            value=""
            onChange={(e) => {
              const language = languages.find(
                (l) => l.code === e.currentTarget.value,
              );
              if (!language) return;
              update((p) => {
                p.navigations[id].languages[language.code] = {
                  direction: language.direction,
                  label: language.label,
                  items: [],
                };
              });
              setLocale(language.code);
            }}
          >
            <option value="">Choose a configured language</option>
            {languages
              .filter(
                (l) => l.code !== family.language && !family.languages[l.code],
              )
              .map((language) => (
                <option value={language.code}>{language.label}</option>
              ))}
          </SelectField>
          {active && (
            <Button
              variant="secondary"
              onClick={() => {
                update((p) => delete p.navigations[id].languages[active]);
                setLocale("");
              }}
            >
              Remove language variant
            </Button>
          )}
          <Button
            variant="secondary"
            disabled={referenced}
            onClick={() => update((p) => delete p.navigations[id])}
          >
            Remove navigation family
          </Button>
          {referenced && (
            <p>Change nodes using this family before removing it.</p>
          )}
        </>
      )}
    </Disclosure>
  );
}
