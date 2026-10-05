'use strict';
for (const link of document.querySelectorAll('.sidebar nav a')) {
  const href = link.getAttribute('href');
  const section = location.pathname.startsWith('/admin/users') ? '/admin/settings' : (location.pathname.startsWith('/admin/migration') || location.pathname.startsWith('/admin/integrations')) ? '/admin/operations' : location.pathname;
  if (href === section || (href.startsWith('/admin/') && section.startsWith(href + '/'))) link.setAttribute('aria-current', 'page');
}
const editor = document.querySelector('[data-editor][data-autosave]');
if (editor) {
  const seo = editor.querySelector('[data-seo-json]');
  if (seo) editor.addEventListener('input', () => { seo.value = JSON.stringify({title:editor.querySelector('[data-seo-title]').value,description:editor.querySelector('[data-seo-description]').value,noindex:editor.querySelector('[data-seo-noindex]').checked,schema_type:editor.querySelector('[data-seo-type]').value}); });
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
        if (!dirty) editor.dispatchEvent(new Event('wpalt:saved'));
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

// Native date/time inputs share an epoch transport with CLI/configuration.
for (const input of document.querySelectorAll('[data-epoch-for]')) {
  const hidden=input.form?.elements.namedItem(input.dataset.epochFor);
  if(!hidden)continue;
  const pad=value=>String(value).padStart(2,'0');
  if(Number(hidden.value)>0){const date=new Date(Number(hidden.value)*1000);input.value=`${date.getFullYear()}-${pad(date.getMonth()+1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;}
  input.addEventListener('input',()=>{hidden.value=input.value?String(Math.floor(new Date(input.value).getTime()/1000)):'0';});
}


// Keep selected local source/visibility on failure and prevent duplicate native jobs.
for (const form of document.querySelectorAll('[data-local-video]')) {
  const button = form.querySelector('button'), status = form.querySelector('[data-video-status]');
  let busy = false;
  form.addEventListener('submit', async event => {
    event.preventDefault();
    if (busy) return;
    busy = true;button.disabled = true;form.setAttribute('aria-busy','true');
    status.hidden = false;status.className = 'notice';status.setAttribute('role','status');
    status.textContent = 'Processing locally… Keep this page open. Your source is retained if processing fails.';
    try {
      const response = await fetch(form.action,{method:'POST',body:new FormData(form),credentials:'same-origin',redirect:'manual',headers:{Accept:'application/json'}});
      if (response.type === 'opaqueredirect') {location.assign('/admin/media');return;}
      let message = 'Processing did not complete. Inspect the media library before retrying if the connection was lost.';
      try {const result = await response.json();if(typeof result.error === 'string')message = result.error;} catch {}
      status.textContent = message;status.className = 'notice error';status.setAttribute('role','alert');
    } catch {
      status.textContent = 'Connection interrupted. Inspect the media library before retrying; processing may have completed.';
      status.className = 'notice error';status.setAttribute('role','alert');
    } finally {
      busy = false;button.disabled = false;form.removeAttribute('aria-busy');
    }
  });
}

// Privacy workflows keep review text on failed requests; never retain a password.
for (const form of document.querySelectorAll('[data-privacy-submit]')) {
  const submit = form.querySelector('button[type="submit"],button:not([type])');
  const status = document.createElement('p');status.className='notice';status.hidden=true;status.setAttribute('role','status');status.setAttribute('aria-live','polite');form.append(status);
  let busy=false;
  form.addEventListener('submit',async event=>{
    event.preventDefault();if(busy)return;busy=true;submit.disabled=true;form.setAttribute('aria-busy','true');status.hidden=false;status.className='notice';status.setAttribute('role','status');status.textContent='Saving your request…';
    const body=new URLSearchParams(new FormData(form));
    for(const password of form.querySelectorAll('input[type=password]'))password.value='';
    try{
      const response=await fetch(form.action,{method:'POST',body,credentials:'same-origin',redirect:'manual',headers:{Accept:'application/json'}});
      if(response.type==='opaqueredirect'){location.assign(location.pathname);return;}
      let message='The request did not complete. Inspect the current record before retrying if the connection was interrupted.';
      try{const result=await response.json();if(typeof result.error==='string')message=result.error;}catch{}
      status.textContent=message;status.className='notice error';status.setAttribute('role','alert');
    }catch{status.textContent='Connection interrupted. Review the current record before retrying; your handling notes remain here.';status.className='notice error';status.setAttribute('role','alert');}
    finally{busy=false;submit.disabled=false;form.removeAttribute('aria-busy');}
  });
}
