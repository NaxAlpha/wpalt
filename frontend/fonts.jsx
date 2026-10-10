import { h, Fragment } from "preact";
import { useState } from "preact/hooks";
import { Button, Field, SelectField, Disclosure, Notice } from "./ui.jsx";

export function FontFaces({ pkg, assets, update }) {
  const [name, setName] = useState("");
  const [asset, setAsset] = useState(assets[0]?.id || "");
  const faces = pkg.fonts || {};
  const selected = assets.find((value) => value.id === asset);
  const valid = /^[a-z][a-z0-9_-]{0,39}$/.test(name) && !faces[name];
  return (
    <Disclosure summary="Local typography" collapseOnNarrow>
      <p>
        <a href="/admin/design-assets" target="_blank" rel="noopener">
          Open local font library in a new tab
        </a>
      </p>
      <p class="help">
        Your current draft stays open. After uploading a font, save the draft
        and reload the Studio to refresh its library.
      </p>
      <SelectField
        label="Site typeface"
        value={pkg.tokens.font}
        onChange={(e) => update((p) => (p.tokens.font = e.currentTarget.value))}
      >
        <option value="system">System sans serif</option>
        <option value="serif">System serif</option>
        <option value="mono">System monospace</option>
        {Object.keys(faces).map((key) => (
          <option value={`local:${key}`}>{key} · local font</option>
        ))}
      </SelectField>
      {Object.entries(faces).map(([key, face]) => (
        <Disclosure summary={`${key} · ${face.weight} ${face.style}`}>
          <p class="help">
            {assets.find((value) => value.id === face.asset)?.label ||
              face.asset}
          </p>
          <SelectField
            label={`${key} loading`}
            value={face.display}
            onChange={(e) =>
              update((p) => (p.fonts[key].display = e.currentTarget.value))
            }
          >
            <option value="swap">Swap when ready</option>
            <option value="optional">Optional on slow connections</option>
          </SelectField>
          <SelectField
            label={`${key} fallback`}
            value={face.fallback}
            onChange={(e) =>
              update((p) => (p.fonts[key].fallback = e.currentTarget.value))
            }
          >
            <option value="system">System sans serif</option>
            <option value="serif">System serif</option>
            <option value="mono">System monospace</option>
          </SelectField>
          <Button
            variant="secondary"
            disabled={pkg.tokens.font === `local:${key}`}
            onClick={() => update((p) => delete p.fonts[key])}
          >
            Remove theme face
          </Button>
        </Disclosure>
      ))}
      {assets.length === 0 ? (
        <Notice>
          No local fonts installed. System typefaces remain available.
        </Notice>
      ) : (
        <>
          <Field
            label="New font face identifier"
            value={name}
            maxlength="40"
            description="Start with a lowercase letter; use letters, digits, underscores or hyphens."
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <SelectField
            label="Installed font"
            value={asset}
            onChange={(e) => setAsset(e.currentTarget.value)}
          >
            {assets.map((value) => (
              <option value={value.id}>
                {value.label} · {value.inspection.weight}
              </option>
            ))}
          </SelectField>
          <Button
            variant="secondary"
            disabled={!valid || !selected || Object.keys(faces).length >= 8}
            onClick={() => {
              update((p) => {
                p.fonts ||= {};
                p.fonts[name] = {
                  asset: selected.id,
                  weight: selected.inspection.weight,
                  style: selected.inspection.italic ? "italic" : "normal",
                  display: "swap",
                  fallback: "system",
                };
              });
              setName("");
            }}
          >
            Add local font face
          </Button>
        </>
      )}
    </Disclosure>
  );
}
