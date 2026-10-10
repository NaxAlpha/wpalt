import { h } from "preact";
import { useState } from "preact/hooks";
import { Disclosure } from "./ui.jsx";
export function ThemePortability({ id, dirty, busy, action, api, reload }) {
  const [rights, confirm] = useState(false);
  return (
    <Disclosure summary="Portable theme and fonts">
      <p>
        Bundles contain the saved native theme, exactly its local fonts and
        their provenance/license text. Image, content and form references must
        already exist on the destination; full site recovery includes the
        complete graph.
      </p>
      {!dirty && id && (
        <a
          class="button secondary"
          href={`/api/admin/design/${id}/bundle?draft=true`}
        >
          Download saved theme bundle
        </a>
      )}
      {dirty && (
        <p>Save your draft before exporting its font-inclusive bundle.</p>
      )}
      <label>
        <input
          type="checkbox"
          checked={rights}
          onChange={(e) => confirm(e.currentTarget.checked)}
        />{" "}
        I have reviewed the bundled fonts' licenses and have permission to
        distribute them.
      </label>
      <label class="file-button">
        Import theme bundle as a new draft
        <input
          type="file"
          accept="application/json"
          disabled={!rights || busy || dirty}
          onChange={(e) => {
            const file = e.currentTarget.files[0];
            e.currentTarget.value = "";
            if (!file) return;
            action(async () => {
              if (file.size > 24 * 1024 * 1024)
                throw new Error("Theme bundles are limited to 24 MiB.");
              const bundle = JSON.parse(await file.text());
              const name = "bundle-" + crypto.randomUUID().slice(0, 8);
              await api(`/api/admin/design/${name}/bundle`, {
                version: 0,
                publish: false,
                rights: true,
                bundle,
              });
              await reload(name);
            });
          }}
        />
      </label>
    </Disclosure>
  );
}
