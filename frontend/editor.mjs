import { Schema } from "prosemirror-model";
import {
  EditorState,
  NodeSelection,
  TextSelection,
  Selection,
} from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { history, undo, redo, closeHistory } from "prosemirror-history";
import {
  baseKeymap,
  toggleMark,
  setBlockType,
  wrapIn,
  chainCommands,
  exitCode,
} from "prosemirror-commands";
import { keymap } from "prosemirror-keymap";
import {
  addListNodes,
  wrapInList,
  splitListItem,
  liftListItem,
  sinkListItem,
} from "prosemirror-schema-list";
import {
  tableNodes,
  tableEditing,
  addRowAfter,
  addColumnAfter,
  deleteRow,
  deleteColumn,
  goToNextCell,
} from "prosemirror-tables";
import {
  inputRules,
  wrappingInputRule,
  textblockTypeInputRule,
} from "prosemirror-inputrules";

const safeUrl = (s) => {
  if (!s || /[\\\u0000-\u0020]/u.test(s) || s.startsWith("//")) return false;
  if (s.startsWith("/")) return true;
  try {
    const u = new URL(s);
    return (
      ["http:", "https:"].includes(u.protocol) && !u.username && !u.password
    );
  } catch {
    return false;
  }
};
const nodes = {
  doc: { content: "block+" },
  paragraph: {
    content: "inline*",
    group: "block",
    parseDOM: [{ tag: "p" }],
    toDOM: () => ["p", 0],
  },
  heading: {
    attrs: { level: { default: 2 } },
    content: "inline*",
    group: "block",
    defining: true,
    parseDOM: [1, 2, 3, 4, 5, 6].map((level) => ({
      tag: `h${level}`,
      attrs: { level },
    })),
    toDOM: (n) => [`h${n.attrs.level}`, 0],
  },
  blockquote: {
    content: "block+",
    group: "block",
    defining: true,
    parseDOM: [{ tag: "blockquote" }],
    toDOM: () => ["blockquote", 0],
  },
  callout: {
    content: "block+",
    group: "block",
    defining: true,
    parseDOM: [{ tag: "aside.callout" }],
    toDOM: () => ["aside", { class: "callout" }, 0],
  },
  code_block: {
    content: "text*",
    marks: "",
    group: "block",
    code: true,
    defining: true,
    attrs: { params: { default: "" } },
    parseDOM: [{ tag: "pre", preserveWhitespace: "full" }],
    toDOM: () => ["pre", ["code", 0]],
  },
  horizontal_rule: {
    group: "block",
    parseDOM: [{ tag: "hr" }],
    toDOM: () => ["hr"],
  },
  text: { group: "inline" },
  hard_break: {
    inline: true,
    group: "inline",
    selectable: false,
    parseDOM: [{ tag: "br" }],
    toDOM: () => ["br"],
  },
  image: {
    inline: true,
    group: "inline",
    draggable: true,
    attrs: { src: {}, alt: { default: "" }, title: { default: null } },
    parseDOM: [
      {
        tag: "img[src]",
        getAttrs: (dom) =>
          safeUrl(dom.getAttribute("src"))
            ? {
                src: dom.getAttribute("src"),
                alt: dom.getAttribute("alt") || "",
              }
            : false,
      },
    ],
    toDOM: (n) => ["img", { ...n.attrs, loading: "lazy" }],
  },
};
// OrderedMap permits the list helper's schema extension without a second schema.
const base = new Schema({
  nodes,
  marks: {
    strong: {
      parseDOM: [
        { tag: "strong" },
        { tag: "b" },
        { style: "font-weight=bold" },
      ],
      toDOM: () => ["strong", 0],
    },
    em: {
      parseDOM: [{ tag: "em" }, { tag: "i" }, { style: "font-style=italic" }],
      toDOM: () => ["em", 0],
    },
    strike: { parseDOM: [{ tag: "s" }, { tag: "del" }], toDOM: () => ["s", 0] },
    code: { parseDOM: [{ tag: "code" }], toDOM: () => ["code", 0] },
    link: {
      attrs: { href: {}, title: { default: null } },
      inclusive: false,
      parseDOM: [
        {
          tag: "a[href]",
          getAttrs: (dom) =>
            safeUrl(dom.getAttribute("href"))
              ? {
                  href: dom.getAttribute("href"),
                  title: dom.getAttribute("title"),
                }
              : false,
        },
      ],
      toDOM: (n) => ["a", { ...n.attrs, rel: "noopener noreferrer" }, 0],
    },
  },
});
const schema = new Schema({
  nodes: addListNodes(base.spec.nodes, "paragraph block*", "block").append(
    tableNodes({ tableGroup: "block", cellContent: "block+" }),
  ),
  marks: base.spec.marks,
});
const form = document.querySelector("[data-editor]");
const host = document.querySelector("[data-writing-canvas]");
if (form && host) initialize();
function initialize() {
  const hidden = form.elements.document,
    fallback = form.elements.body;
  // Do not replace text entered while the enhancement bundle was still loading.
  if (fallback.value !== fallback.defaultValue) return;
  let doc;
  try {
    doc = schema.nodeFromJSON(JSON.parse(hidden.value).root);
    doc.check();
  } catch {
    return;
  }
  const ui = document.createElement("div");
  ui.className = "writing-tools";
  ui.setAttribute("role", "group");
  ui.setAttribute("aria-label", "Writing tools");
  host.before(ui);
  const hint = document.createElement("p");
  hint.className = "muted writing-hint";
  hint.id = "writing-help";
  hint.textContent =
    "Write naturally. Type / in an empty paragraph to choose a block. Use ⌘/Ctrl B or I to format.";
  host.before(hint);
  const notice = document.createElement("div");
  notice.className = "notice";
  notice.hidden = true;
  notice.setAttribute("role", "status");
  host.before(notice);
  const menu = document.createElement("div");
  menu.className = "insertion-menu";
  menu.hidden = true;
  menu.setAttribute("role", "group");
  menu.setAttribute("aria-label", "Insert or convert block");
  ui.after(menu);
  const dialog = document.createElement("dialog");
  dialog.className = "writing-dialog";
  form.after(dialog);
  const recoveryKey = `wpalt:authoring:${form.dataset.owner}:${form.getAttribute("action")}`;
  const recoveryVersion = Number(form.elements.version.value);
  let writeTimer,
    slash = false,
    destroyed = false;
  const dispatch = (tr) => view.dispatch(tr);
  function syncSelection() {
    if (view.composing) return;
    const selection = window.getSelection();
    if (
      !selection?.anchorNode ||
      !selection.focusNode ||
      !view.dom.contains(selection.anchorNode) ||
      !view.dom.contains(selection.focusNode)
    )
      return;
    try {
      const anchor = view.posAtDOM(
        selection.anchorNode,
        selection.anchorOffset,
      );
      const head = view.posAtDOM(selection.focusNode, selection.focusOffset);
      if (
        view.state.selection.anchor === anchor &&
        view.state.selection.head === head
      )
        return;
      const tr = view.state.tr.setSelection(
        TextSelection.between(
          view.state.doc.resolve(anchor),
          view.state.doc.resolve(head),
        ),
      );
      view.dispatch(tr);
    } catch {
      /* Detached browser selection leaves the last valid engine selection intact. */
    }
  }
  const run = (command) => {
    syncSelection();
    command(view.state, dispatch, view);
    view.focus();
  };
  // Document-edge navigation is a transaction, not a browser selection followed
  // by a delayed observer update. This also keeps rapid post-composition keys
  // from racing with native caret reconciliation.
  const edge =
    (direction, extend = false) =>
    (state, send) => {
      const destination =
        direction < 0
          ? Selection.atStart(state.doc)
          : Selection.atEnd(state.doc);
      const selection = extend
        ? TextSelection.between(state.selection.$anchor, destination.$head)
        : destination;
      if (send) send(state.tr.setSelection(selection).scrollIntoView());
      return true;
    };
  const mac = /Mac|iPhone|iPad/u.test(navigator.platform);
  const edgeKeys = {
    "Mod-Home": edge(-1),
    "Mod-End": edge(1),
    "Mod-Shift-Home": edge(-1, true),
    "Mod-Shift-End": edge(1, true),
    ...(mac
      ? {
          "Mod-ArrowUp": edge(-1),
          "Mod-ArrowDown": edge(1),
          "Mod-Shift-ArrowUp": edge(-1, true),
          "Mod-Shift-ArrowDown": edge(1, true),
        }
      : {}),
  };
  const view = new EditorView(host, {
    state: EditorState.create({
      schema,
      doc,
      plugins: [
        history(),
        inputRules({
          rules: [
            wrappingInputRule(/^\s*>\s$/, schema.nodes.blockquote),
            wrappingInputRule(/^\s*[-+*]\s$/, schema.nodes.bullet_list),
            textblockTypeInputRule(
              /^(#{1,6})\s$/,
              schema.nodes.heading,
              (match) => ({ level: match[1].length }),
            ),
          ],
        }),
        keymap({
          ...edgeKeys,
          "Mod-z": undo,
          "Mod-Shift-z": redo,
          "Mod-y": redo,
          "Mod-b": toggleMark(schema.marks.strong),
          "Mod-i": toggleMark(schema.marks.em),
          "Mod-Alt-ArrowUp": () => move(-1),
          "Mod-Alt-ArrowDown": () => move(1),
          Enter: splitListItem(schema.nodes.list_item),
          Tab: chainCommands(
            goToNextCell(1),
            sinkListItem(schema.nodes.list_item),
          ),
          "Shift-Tab": chainCommands(
            goToNextCell(-1),
            liftListItem(schema.nodes.list_item),
          ),
          "Mod-Enter": chainCommands(exitCode, (state, send) => {
            if (send)
              send(
                state.tr
                  .replaceSelectionWith(schema.nodes.hard_break.create())
                  .scrollIntoView(),
              );
            return true;
          }),
        }),
        keymap(baseKeymap),
        tableEditing(),
      ],
    }),
    attributes: {
      role: "textbox",
      "aria-label": "Content",
      "aria-multiline": "true",
      "aria-describedby": "writing-help",
      spellcheck: "true",
      lang: form.elements.locale.value,
      dir: "auto",
    },
    dispatchTransaction(tr) {
      const next = view.state.apply(tr);
      view.updateState(next);
      if (tr.docChanged) {
        hidden.value = JSON.stringify({ version: 1, root: next.doc.toJSON() });
        fallback.value = "";
        form.dispatchEvent(new Event("input", { bubbles: true }));
        clearTimeout(writeTimer);
        writeTimer = setTimeout(storeRecovery, 400);
      }
      updateTools();
      if (!view.composing) {
        const p = next.selection.$from.parent;
        const active =
          p.type === schema.nodes.paragraph && p.textContent === "/";
        if (active && !slash) openMenu();
        if (!active && slash) closeMenu();
        slash = active;
      }
    },
    handleDOMEvents: {
      keydown(_v, event) {
        syncSelection();
        if (!menu.hidden && event.key === "ArrowDown") {
          menu.querySelector("button")?.focus();
          return true;
        }
        if (event.key === "Escape") {
          closeMenu();
          return false;
        }
        return false;
      },
    },
    transformPastedHTML(html) {
      const parsed = new window.DOMParser().parseFromString(html, "text/html");
      parsed
        .querySelectorAll(
          "script,style,iframe,object,embed,form,input,button,svg,math",
        )
        .forEach((e) => e.remove());
      parsed.querySelectorAll("a[href],img[src]").forEach((e) => {
        const a = e.tagName === "A" ? "href" : "src";
        if (
          !safeUrl(e.getAttribute(a)) ||
          (a === "src" && !e.getAttribute(a).startsWith("/media/"))
        )
          e.removeAttribute(a);
      });
      return parsed.body.innerHTML;
    },
  });
  hidden.disabled = false;
  form.elements.import_markdown.checked = false;
  form.elements.import_markdown.disabled = true;
  form.querySelector("[data-markdown-replacement]").hidden = true;
  fallback.value = "";
  form.elements.locale.addEventListener("change", () =>
    view.setProps({
      attributes: {
        ...view.props.attributes,
        lang: form.elements.locale.value,
        dir: "auto",
      },
    }),
  );
  form.addEventListener("input", () => {
    clearTimeout(writeTimer);
    writeTimer = setTimeout(storeRecovery, 400);
  });
  fallback.closest("label").hidden = true;
  host.hidden = false;
  function button(label, action, target = ui) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "secondary";
    b.textContent = label;
    if (["Apply link", "Insert image"].includes(label))
      b.dataset.dialogConfirm = "";
    b.addEventListener("mousedown", (e) => {
      syncSelection();
      if (!b.draggable) e.preventDefault();
    });
    b.addEventListener("click", action);
    target.append(b);
    return b;
  }
  const formatting = document.createElement("div");
  formatting.className = "writing-formatting";
  formatting.setAttribute("role", "group");
  formatting.setAttribute("aria-label", "Selection formatting");
  formatting.hidden = true;
  ui.after(formatting);
  let manualFormatting = false;
  const formatButton = button("Format", () => {
    formatting.hidden = !formatting.hidden;
    manualFormatting = !formatting.hidden;
    formatButton.setAttribute("aria-expanded", String(!formatting.hidden));
  });
  formatButton.setAttribute("aria-expanded", "false");
  const markButtons = [];
  for (const [label, type] of [
    ["Bold", "strong"],
    ["Italic", "em"],
    ["Strike", "strike"],
    ["Inline code", "code"],
  ]) {
    const b = button(
      label,
      () => run(toggleMark(schema.marks[type])),
      formatting,
    );
    b.setAttribute("aria-pressed", "false");
    markButtons.push([b, type]);
  }
  const linkButton = button("Link", () => linkDialog(), formatting);
  button("Blocks", () => {
    slash =
      view.state.selection.$from.parent.type === schema.nodes.paragraph &&
      view.state.selection.$from.parent.textContent === "/";
    openMenu();
  });
  button("Undo", () => run(undo));
  button("Redo", () => run(redo));
  const arrange = document.createElement("div");
  arrange.className = "writing-arrange";
  arrange.setAttribute("role", "group");
  arrange.setAttribute("aria-label", "Current block");
  ui.after(arrange);
  arrange.hidden = true;
  const organizeButton = button("Organize", () => {
    arrange.hidden = !arrange.hidden;
    organizeButton.setAttribute("aria-expanded", String(!arrange.hidden));
  });
  organizeButton.setAttribute("aria-expanded", "false");
  button(
    "New paragraph",
    () => {
      if (slash) {
        const { $from } = view.state.selection;
        view.dispatch(view.state.tr.delete($from.start(), $from.end()));
        slash = false;
        closeMenu();
        view.focus();
        return;
      }
      closeMenu();
      const current = positions()[topIndex(view.state.selection.from)];
      if (!current) return;
      const pos = current.offset + current.node.nodeSize;
      const tr = view.state.tr.insert(pos, schema.nodes.paragraph.create());
      tr.setSelection(TextSelection.near(tr.doc.resolve(pos + 1)));
      view.dispatch(closeHistory(tr).scrollIntoView());
      view.focus();
    },
    menu,
  );
  const handle = button(
    "Drag block",
    () => {
      notice.hidden = false;
      notice.textContent =
        "Drag this handle to move the selected top-level block, or use Move up/down. Keyboard shortcut: Ctrl/⌘ Alt ↑/↓.";
    },
    arrange,
  );
  // Pointer capture supports mouse/touch and avoids native HTML button drag differences.
  handle.style.touchAction = "none";
  handle.style.cursor = "grab";
  let drag = null;
  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    drag = {
      id: event.pointerId,
      source: topIndex(view.state.selection.from),
      x: event.clientX,
      y: event.clientY,
      moved: false,
    };
    handle.setPointerCapture(event.pointerId);
  });
  handle.addEventListener("pointermove", (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    if (Math.hypot(event.clientX - drag.x, event.clientY - drag.y) > 5) {
      drag.moved = true;
      handle.style.cursor = "grabbing";
    }
  });
  handle.addEventListener("pointerup", (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const current = drag;
    drag = null;
    handle.style.cursor = "grab";
    handle.releasePointerCapture(event.pointerId);
    if (current.moved) {
      const point = view.posAtCoords({
        left: event.clientX,
        top: event.clientY,
      });
      if (point) reorder(current.source, topIndex(point.pos));
    }
  });
  handle.addEventListener("pointercancel", () => {
    drag = null;
    handle.style.cursor = "grab";
  });
  button("Move up", () => move(-1), arrange);
  button("Move down", () => move(1), arrange);
  button("Duplicate", () => duplicate(), arrange);
  const context = document.createElement("div");
  context.className = "writing-table-tools";
  context.hidden = true;
  context.setAttribute("role", "group");
  context.setAttribute("aria-label", "Table tools");
  arrange.after(context);
  for (const [label, command] of [
    ["Add row", addRowAfter],
    ["Add column", addColumnAfter],
    ["Delete row", deleteRow],
    ["Delete column", deleteColumn],
  ])
    button(label, () => run(command), context);
  for (const [label, kind, attrs] of [
    ["Text", "paragraph"],
    ["Heading 1", "heading", { level: 1 }],
    ["Heading 2", "heading", { level: 2 }],
    ["Heading 3", "heading", { level: 3 }],
    ["Bullet list", "bullet_list"],
    ["Numbered list", "ordered_list"],
    ["Quote", "blockquote"],
    ["Callout", "callout"],
    ["Code", "code_block"],
    ["Image", "image"],
    ["Table", "table"],
    ["Divider", "horizontal_rule"],
  ])
    button(
      label,
      () => {
        if (slash) {
          const { $from } = view.state.selection;
          view.dispatch(view.state.tr.delete($from.start(), $from.end()));
        }
        slash = false;
        closeMenu();
        if (kind === "image") return imageDialog();
        if (kind === "table") return insertTable();
        if (["paragraph", "heading", "code_block"].includes(kind))
          run(setBlockType(schema.nodes[kind], attrs));
        else if (kind === "bullet_list" || kind === "ordered_list")
          run(wrapInList(schema.nodes[kind]));
        else if (kind === "horizontal_rule")
          run((state, send) => {
            send(
              state.tr
                .replaceSelectionWith(schema.nodes.horizontal_rule.create())
                .scrollIntoView(),
            );
            return true;
          });
        else run(wrapIn(schema.nodes[kind]));
      },
      menu,
    );
  menu.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu();
      view.focus();
    } else if (["ArrowDown", "ArrowUp"].includes(event.key)) {
      event.preventDefault();
      const choices = [...menu.querySelectorAll("button")];
      const current = choices.indexOf(document.activeElement);
      choices[
        (current + (event.key === "ArrowDown" ? 1 : -1) + choices.length) %
          choices.length
      ]?.focus();
    }
  });
  function openMenu() {
    menu.hidden = false;
  }
  function closeMenu() {
    menu.hidden = true;
  }
  function topIndex(pos) {
    return view.state.doc.resolve(pos).index(0);
  }
  function positions() {
    const list = [];
    view.state.doc.forEach((node, offset, index) =>
      list.push({ node, offset, index }),
    );
    return list;
  }
  function reorder(source, target) {
    const list = positions();
    if (
      source === target ||
      source < 0 ||
      target < 0 ||
      source >= list.length ||
      target >= list.length
    )
      return false;
    const item = list[source];
    let targetPos = list[target].offset;
    if (source < target)
      targetPos += list[target].node.nodeSize - item.node.nodeSize;
    const tr = view.state.tr
      .delete(item.offset, item.offset + item.node.nodeSize)
      .insert(targetPos, item.node);
    tr.setSelection(NodeSelection.create(tr.doc, targetPos));
    view.dispatch(closeHistory(tr).scrollIntoView());
    view.focus();
    return true;
  }
  function move(direction) {
    syncSelection();
    const current = topIndex(view.state.selection.from);
    return reorder(current, current + direction);
  }
  function duplicate() {
    syncSelection();
    const list = positions(),
      current = list[topIndex(view.state.selection.from)];
    if (!current) return;
    const pos = current.offset + current.node.nodeSize;
    const tr = view.state.tr.insert(pos, current.node);
    tr.setSelection(NodeSelection.create(tr.doc, pos));
    view.dispatch(closeHistory(tr).scrollIntoView());
    view.focus();
  }
  function insertTable() {
    const cell = () =>
      schema.nodes.table_cell.create(
        { colspan: 1, rowspan: 1, colwidth: null },
        schema.nodes.paragraph.create(),
      );
    const rows = [0, 1].map(() =>
      schema.nodes.table_row.create(null, [cell(), cell()]),
    );
    run((state, send) => {
      send(
        state.tr
          .replaceSelectionWith(schema.nodes.table.create(null, rows))
          .scrollIntoView(),
      );
      return true;
    });
  }
  function openDialog(title, build) {
    dialog.replaceChildren();
    dialog.setAttribute("aria-label", title);
    const h = document.createElement("h2");
    h.textContent = title;
    dialog.append(h);
    const f = document.createElement("form");
    f.method = "dialog";
    dialog.append(f);
    build(f);
    f.addEventListener("submit", (event) => event.preventDefault());
    f.addEventListener("keydown", (event) => {
      if (
        event.key === "Enter" &&
        event.target.tagName === "INPUT" &&
        !event.isComposing
      ) {
        event.preventDefault();
        f.querySelector("[data-dialog-confirm]")?.click();
      }
    });
    button(
      "Cancel",
      () => {
        dialog.close();
        view.focus();
      },
      f,
    );
    dialog.showModal();
    f.querySelector("input,select")?.focus();
  }
  function field(f, label, value = "") {
    const l = document.createElement("label");
    l.textContent = label;
    const i = document.createElement("input");
    i.value = value;
    l.append(i);
    f.append(l);
    return i;
  }
  function linkDialog() {
    const selected = view.state.selection;
    const mark = view.state.storedMarks || selected.$from.marks();
    const current = mark.find((m) => m.type === schema.marks.link);
    openDialog("Edit link", (f) => {
      const url = field(f, "Link URL", current?.attrs.href || "");
      const error = document.createElement("p");
      error.setAttribute("role", "alert");
      f.append(error);
      button(
        "Apply link",
        () => {
          if (!safeUrl(url.value)) {
            error.textContent = "Use a local path or an HTTP(S) URL.";
            return;
          }
          dialog.close();
          run((state, send) => {
            send(
              state.tr.addMark(
                selected.from,
                selected.to,
                schema.marks.link.create({ href: url.value, title: null }),
              ),
            );
            return true;
          });
        },
        f,
      );
      button(
        "Remove link",
        () => {
          dialog.close();
          run((state, send) => {
            send(
              state.tr.removeMark(
                selected.from,
                selected.to,
                schema.marks.link,
              ),
            );
            return true;
          });
        },
        f,
      );
    });
  }
  function imageDialog() {
    const selection = view.state.selection;
    openDialog("Insert image", (f) => {
      const src = field(f, "Media URL", "/media/");
      const alt = field(f, "Image description");
      const info = document.createElement("p");
      info.textContent =
        "Choose an uploaded image below, or enter its /media/ID URL. Private images remain protected.";
      f.append(info);
      const picker = document.createElement("div");
      picker.className = "image-picker";
      picker.setAttribute("role", "group");
      picker.setAttribute("aria-label", "Uploaded images");
      f.append(picker);
      const loading = document.createElement("p");
      loading.setAttribute("role", "status");
      loading.textContent = "Loading your images…";
      picker.append(loading);
      async function loadMedia(after = "") {
        picker.setAttribute("aria-busy", "true");
        try {
          const response = await fetch(
            "/api/admin/media" +
              (after ? "?after=" + encodeURIComponent(after) : ""),
          );
          if (!response.ok) throw Error();
          const data = await response.json();
          if (!f.isConnected) return;
          loading.textContent = data.items.length
            ? "Choose an image. Private images are visible only to signed-in editors."
            : "No more images. Upload in Media library.";
          for (const item of data.items) {
            const b = button(
              "",
              () => {
                src.value = "/media/" + item.id;
                alt.value = item.alt || item.name;
              },
              picker,
            );
            const img = document.createElement("img");
            img.src = "/media/" + item.id;
            img.alt = "";
            img.loading = "lazy";
            b.append(
              img,
              document.createTextNode(
                (item.alt || item.name) + " · " + item.visibility,
              ),
            );
          }
          if (data.next) {
            const more = button(
              "More images",
              () => {
                more.remove();
                loadMedia(data.next);
              },
              picker,
            );
          }
        } catch {
          loading.textContent =
            "Image list is unavailable. Retry by reopening, or use a known uploaded URL.";
        } finally {
          picker.setAttribute("aria-busy", "false");
        }
      }
      loadMedia();
      const a = document.createElement("a");
      a.href = "/admin/media";
      a.target = "_blank";
      a.rel = "noopener";
      a.textContent = "Open Media library";
      f.append(a);
      const error = document.createElement("p");
      error.setAttribute("role", "alert");
      f.append(error);
      button(
        "Insert image",
        () => {
          if (
            !/^\/media\/[0-9a-f-]{36}$/.test(src.value) ||
            !alt.value.trim()
          ) {
            error.textContent =
              "Choose an uploaded media URL and a useful description.";
            return;
          }
          dialog.close();
          run((state, send) => {
            send(
              state.tr
                .setSelection(selection)
                .replaceSelectionWith(
                  schema.nodes.image.create({
                    src: src.value,
                    alt: alt.value,
                    title: null,
                  }),
                )
                .scrollIntoView(),
            );
            return true;
          });
        },
        f,
      );
    });
  }
  function updateTools() {
    const selected =
      !view.state.selection.empty &&
      !(view.state.selection instanceof NodeSelection);
    formatting.hidden = !(manualFormatting || selected);
    formatButton.setAttribute("aria-expanded", String(!formatting.hidden));
    linkButton.disabled = view.state.selection.empty;
    for (const [b, type] of markButtons) {
      const { from, to, empty, $from } = view.state.selection;
      b.setAttribute(
        "aria-pressed",
        String(
          empty
            ? !!schema.marks[type].isInSet(
                view.state.storedMarks || $from.marks(),
              )
            : view.state.doc.rangeHasMark(from, to, schema.marks[type]),
        ),
      );
    }
    context.hidden = !view.state.selection.$from.path.some(
      (x) => x?.type === schema.nodes.table,
    );
  }
  function storeRecovery() {
    try {
      localStorage.setItem(
        recoveryKey,
        JSON.stringify({
          version: Number(form.elements.version.value),
          document: hidden.value,
          fields: Object.fromEntries(
            [
              ...new FormData(form),
              ...Array.from(
                form.querySelectorAll(
                  "input[type=checkbox][name]:not(:disabled)",
                ),
                (el) => [el.name, String(el.checked)],
              ),
            ].filter(
              ([key]) =>
                !["csrf", "document", "version", "body", "action"].includes(
                  key,
                ),
            ),
          ),
          at: Date.now(),
        }),
      );
    } catch {
      notice.hidden = false;
      notice.textContent =
        "Browser recovery is unavailable. Keep this tab open until the server saves your draft.";
    }
  }
  let saved;
  try {
    saved = JSON.parse(localStorage.getItem(recoveryKey) || "null");
  } catch {}
  let sameRecovery = false;
  if (saved && typeof saved.document === "string") {
    try {
      const tree = schema.nodeFromJSON(JSON.parse(saved.document).root);
      sameRecovery =
        tree.eq(view.state.doc) &&
        Object.entries(saved.fields || {}).every(([key, value]) => {
          const el = form.elements.namedItem(key);
          return (
            !el ||
            (el.type === "checkbox"
              ? String(el.checked) === value
              : el.value === value)
          );
        });
      if (sameRecovery) localStorage.removeItem(recoveryKey);
    } catch {}
  }
  if (saved && !sameRecovery && typeof saved.document === "string") {
    notice.hidden = false;
    notice.textContent =
      saved.version === recoveryVersion
        ? "A browser recovery copy is available. It has not replaced your saved draft."
        : "A browser recovery copy is available from another version. Review it before saving; newer server content has not been replaced.";
    button(
      "Restore recovery copy",
      () => {
        try {
          const recovered = schema.nodeFromJSON(
            JSON.parse(saved.document).root,
          );
          recovered.check();
          const tr = view.state.tr.replaceWith(
            0,
            view.state.doc.content.size,
            recovered.content,
          );
          view.dispatch(closeHistory(tr));
          for (const [key, value] of Object.entries(saved.fields || {})) {
            const el = form.elements.namedItem(key);
            if (
              el &&
              key !== "csrf" &&
              key !== "version" &&
              key !== "import_markdown" &&
              !(el instanceof RadioNodeList)
            ) {
              if (el.type === "checkbox") el.checked = value === "true";
              else el.value = value;
            }
          }
          form.dispatchEvent(new Event("wpalt:recovery"));
          form.elements.locale.dispatchEvent(
            new Event("change", { bubbles: true }),
          );
          form.elements.kind?.dispatchEvent(
            new Event("change", { bubbles: true }),
          );
          form.dispatchEvent(new Event("input", { bubbles: true }));
          notice.textContent =
            "Recovery copy loaded as unsaved work. Review it, then save deliberately.";
        } catch {
          notice.textContent =
            "Recovery copy could not be read. Your saved draft is unchanged.";
        }
      },
      notice,
    );
    button(
      "Discard recovery copy",
      () => {
        try {
          localStorage.removeItem(recoveryKey);
        } catch {}
        notice.hidden = true;
      },
      notice,
    );
  }
  form.addEventListener("wpalt:saved", () => {
    try {
      localStorage.removeItem(recoveryKey);
    } catch {}
  });
  form.addEventListener("submit", () => {
    storeRecovery();
  });
  window.addEventListener("pagehide", () => {
    if (!destroyed) {
      clearTimeout(writeTimer);
      view.destroy();
      destroyed = true;
    }
  });
  updateTools();
}
