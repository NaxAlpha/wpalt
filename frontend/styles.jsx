import { h, Fragment } from "preact";
import { useState } from "preact/hooks";
import { Button, Field, SelectField, Disclosure, Notice } from "./ui.jsx";
export function CompositionStyle({ pkg, node, changeNode, update }) {
  const [name, setName] = useState("");
  const shared = node.style_ref || "";
  const style = shared ? pkg.styles?.[shared] || {} : node.style || {};
  const edit = (key, value) =>
    update((p) => {
      if (shared) p.styles[shared][key] = value;
      else
        changeNode(p).style = { ...(changeNode(p).style || {}), [key]: value };
    });
  const numeric = [
    ["columns", 0, 6],
    ["mobile_columns", 0, 3],
    ["gap", 0, 64],
    ["padding", 0, 96],
    ["width", 0, 1600],
    ["font_size", 0, 96],
    ["font_weight", 0, 900],
    ["line_height", 0, 240],
    ["margin_block", 0, 96],
    ["border_width", 0, 8],
    ["radius", 0, 48],
  ];
  return (
    <Disclosure summary="Layout and typography" initialOpen>
      <SelectField
        label="Reusable style"
        value={shared}
        onChange={(e) =>
          update((p) => {
            const current = changeNode(p);
            const next = e.currentTarget.value;
            if (!next && current.style_ref)
              current.style = JSON.parse(
                JSON.stringify(p.styles[current.style_ref]),
              );
            else if (next) current.style = {};
            current.style_ref = next;
          })
        }
      >
        <option value="">Independent node style</option>
        {Object.keys(pkg.styles || {}).map((key) => (
          <option value={key}>{key}</option>
        ))}
      </SelectField>
      {shared && (
        <Notice>
          Editing {shared} updates every node using this style in the theme
          draft. Choose independent node style to detach a copy.
        </Notice>
      )}
      {!shared && (
        <>
          <Field
            label="New reusable style name"
            value={name}
            maxlength="40"
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <Button
            variant="secondary"
            disabled={
              !/^[a-z][a-z0-9_-]{0,39}$/.test(name) ||
              !!pkg.styles?.[name] ||
              Object.keys(pkg.styles || {}).length >= 32
            }
            onClick={() => {
              update((p) => {
                p.styles ||= {};
                p.styles[name] = JSON.parse(
                  JSON.stringify(changeNode(p).style || {}),
                );
                changeNode(p).style = {};
                changeNode(p).style_ref = name;
              });
              setName("");
            }}
          >
            Save as reusable style
          </Button>
        </>
      )}
      <SelectField
        label="Layout"
        value={style.layout || ""}
        onChange={(e) => edit("layout", e.currentTarget.value)}
      >
        {["", "stack", "grid", "row"].map((value) => (
          <option value={value}>{value || "Default"}</option>
        ))}
      </SelectField>
      <SelectField
        label="Node typeface"
        value={style.font || ""}
        onChange={(e) => edit("font", e.currentTarget.value)}
      >
        <option value="">Inherit</option>
        <option value="system">System sans serif</option>
        <option value="serif">System serif</option>
        <option value="mono">System monospace</option>
        {Object.keys(pkg.fonts || {}).map((key) => (
          <option value={`local:${key}`}>{key} · local font</option>
        ))}
      </SelectField>
      <p class="help">
        Zero inherits a value. Type size is 12–96px, weight 100–900 and line
        height 100–240%. Spacing uses pixels.
      </p>
      {numeric.map(([key, min, max]) => (
        <Field
          label={key.replaceAll("_", " ")}
          type="number"
          min={min}
          max={max}
          value={style[key] || 0}
          onInput={(e) => edit(key, Number(e.currentTarget.value))}
        />
      ))}
      <SelectField
        label="Text alignment"
        value={style.align || ""}
        onChange={(e) => edit("align", e.currentTarget.value)}
      >
        {["", "start", "center", "end", "left", "right"].map((value) => (
          <option value={value}>{value || "Inherit"}</option>
        ))}
      </SelectField>
      {["color", "background", "border_color"].map((key) => (
        <div>
          <Field
            label={key.replaceAll("_", " ")}
            type="color"
            value={style[key] || "#000000"}
            onInput={(e) => edit(key, e.currentTarget.value)}
          />
          <Button
            variant="secondary"
            disabled={!style[key]}
            onClick={() => edit(key, "")}
          >
            Reset {key.replaceAll("_", " ")}
          </Button>
        </div>
      ))}
    </Disclosure>
  );
}
