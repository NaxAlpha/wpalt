import { h, render } from "preact";
import { useState } from "preact/hooks";
import { Button, Field, Notice, Disclosure } from "./ui.jsx";
function Gallery() {
  const [value, setValue] = useState("");
  return (
    <main class="workspace ui-gallery">
      <header class="page-heading">
        <p class="eyebrow">Component reference</p>
        <h1>Clear controls. Consistent rhythm.</h1>
        <p>Production primitives, rendered in their meaningful states.</p>
      </header>
      <section class="panel" data-ui="controls">
        <h2>Actions and feedback</h2>
        <div class="toolbar">
          <Button data-ui="primary">Save draft</Button>
          <Button variant="secondary">Preview</Button>
          <Button variant="danger">Delete item</Button>
          <Button disabled>Unavailable</Button>
          <Button busy>Saving…</Button>
        </div>
        <Notice>Draft saved. Published content is unchanged.</Notice>
        <Notice error>Save needs attention. Your edit remains here.</Notice>
      </section>
      <section class="panel" data-ui="fields">
        <h2>Authoring fields</h2>
        <div class="field-row">
          <Field
            label="Title"
            data-ui="title"
            value={value}
            onInput={(e) => setValue(e.currentTarget.value)}
            description="A clear name for your content."
          />
          <Field label="Slug" value="" error="Enter a unique URL slug." />
        </div>
        <label>
          Content model
          <select>
            <option>Project</option>
          </select>
        </label>
        <label class="ui-check">
          <input type="checkbox" /> Featured project
        </label>
      </section>
      <section class="panel">
        <h2>Content states</h2>
        <div class="toolbar">
          <span class="status published">Published</span>
          <span class="status scheduled">Scheduled</span>
          <span class="status">Draft</span>
        </div>
        <Disclosure summary="Revision details">
          <p>A recoverable working draft.</p>
        </Disclosure>
      </section>
    </main>
  );
}
render(<Gallery />, document.getElementById("gallery"));
