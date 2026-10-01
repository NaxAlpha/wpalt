'use strict';
for (const link of document.querySelectorAll('.sidebar nav a')) {
  const href = link.getAttribute('href');
  const section = location.pathname.startsWith('/admin/users') ? '/admin/settings' : location.pathname;
  if (href === section || (href.startsWith('/admin/') && section.startsWith(href + '/'))) link.setAttribute('aria-current', 'page');
}
const editor = document.querySelector('[data-editor]');
if (editor) {
  let dirty = false, saving = false, conflicted = false, timer;
  const status = document.querySelector('[data-save-status]');
  const error = document.querySelector('[data-editor-error]');
  const version = editor.elements.version;
  editor.addEventListener('input', () => {
    dirty = true;
    status.textContent = 'Unsaved changes';
    clearTimeout(timer);
    timer = setTimeout(autosave, 2500);
  });
  async function autosave() {
    if (!dirty || saving || conflicted || editor.dataset.new === 'true') return;
    saving = true; dirty = false;
    const values = new URLSearchParams(new FormData(editor));
    values.set('action', 'autosave');
    status.textContent = 'Saving draft…';
    try {
      const response = await fetch(editor.getAttribute('action'), {method: 'POST', body: values, headers: {'Accept': 'application/json'}});
      const result = await response.json();
      if (!response.ok) {
        dirty = true;
        if (response.status === 409) conflicted = true;
        error.hidden = false; error.textContent = result.error || 'Save failed; your text remains in the editor.';
        status.textContent = 'Not saved';
      } else {
        for (const input of document.querySelectorAll('input[name=version]')) input.value = result.version;
        error.hidden = true;
        status.textContent = dirty ? 'Unsaved changes' : 'Draft saved · live page unchanged';
      }
    } catch (_) {
      dirty = true; status.textContent = 'Offline · changes remain in this editor';
    } finally {
      saving = false;
      if (dirty && !conflicted) timer = setTimeout(autosave, 5000);
    }
  }
  editor.addEventListener('submit', event => {
    clearTimeout(timer);
    if (saving || conflicted) {
      event.preventDefault();
      error.hidden = false;
      error.textContent = saving ? 'An autosave is in progress. Try saving again in a moment.' : 'Another edit changed this item. Copy your changes and reload before saving.';
    } else { dirty = false; }
  });
  window.addEventListener('beforeunload', event => { if (dirty || saving) { event.preventDefault(); event.returnValue = ''; } });
}
for (const input of document.querySelectorAll('[data-schedule-time]')) {
  input.addEventListener('change', () => {
    const target = document.querySelector('[name=publish_at]');
    target.value = input.value ? Math.floor(new Date(input.value).getTime() / 1000) : 0;
  });
}
