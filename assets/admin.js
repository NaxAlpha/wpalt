'use strict';
for (const link of document.querySelectorAll('.sidebar nav a')) {
  const href = link.getAttribute('href');
  const section = location.pathname.startsWith('/admin/users') ? '/admin/settings' : (location.pathname.startsWith('/admin/migration') || location.pathname.startsWith('/admin/integrations')) ? '/admin/operations' : location.pathname;
  if (href === section || (href.startsWith('/admin/') && section.startsWith(href + '/'))) link.setAttribute('aria-current', 'page');
}
const editor = document.querySelector('[data-editor][data-autosave]');
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
    if (editor.dataset.discoveryIncomplete === 'true') { status.textContent = 'Finish the video/news declaration to save your working copy. Your edits remain here.'; return; }
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
        const workflow=result.editorial;
        if(workflow) {
          for(const form of document.querySelectorAll('[data-review-decision]')) {
            form.elements.content_version.value=result.version;
            form.elements.workflow_version.value=workflow.version;
            for(const button of form.querySelectorAll('button')) button.disabled=true;
          }
          const label=editor.querySelector('[data-editorial-state]');
          if(label)label.textContent='Draft changed · request review again';
        }
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
  for(const form of document.querySelectorAll('[data-review-decision]')) form.addEventListener('submit',event=>{
    if(dirty||saving||conflicted){event.preventDefault();error.hidden=false;error.textContent='Save or discard your working edits before deciding on the saved review.';}
  });
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

// Private document preparation preserves the page and verifies before extraction.
for (const form of document.querySelectorAll('[data-attested-document]')) {
  const button = form.querySelector('button');
  const status = form.querySelector('[data-document-status]');
  const downloads = form.querySelector('[data-document-downloads]');
  const format = form.elements.namedItem('kind');
  const urls = [];
  let busy = false;
  const clear = () => {
    for (const url of urls.splice(0)) URL.revokeObjectURL(url);
    downloads.replaceChildren();downloads.hidden = true;
  };
  window.addEventListener('pagehide', clear);
  form.addEventListener('submit', async event => {
    if (!globalThis.crypto?.subtle) return;
    event.preventDefault();if (busy) return;
    const body = new URLSearchParams(new FormData(form));
    busy = true;button.disabled = true;format.disabled = true;form.setAttribute('aria-busy','true');clear();
    status.hidden = false;status.className = 'notice';status.setAttribute('role','status');
    status.textContent = 'Preparing and checking your private document…';
    const abort = new AbortController();const timer = setTimeout(() => abort.abort(),45000);
    try {
      const response = await fetch(form.action,{method:'POST',body,credentials:'same-origin',redirect:'error',signal:abort.signal,headers:{Accept:'application/json'}});
      const reader = response.body.getReader();const chunks = [];let length = 0;
      try {
        for (;;) {
          const {value,done} = await reader.read();if (done) break;
          length += value.length;if (length > 12*1024*1024) throw new Error('Document response exceeds its byte limit.');chunks.push(value);
        }
      } finally {await reader.cancel().catch(() => {});}
      const raw = new Uint8Array(length);let offset = 0;for (const chunk of chunks) {raw.set(chunk,offset);offset += chunk.length;}
      const bundle = JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(raw));
      if (!response.ok) throw new Error(typeof bundle.error === 'string' ? bundle.error : 'Document preparation failed.');
      const invalid = () => new Error('Document verification failed. No artifact was extracted.');
      const hex = bytes => Array.from(bytes,x=>x.toString(16).padStart(2,'0')).join('');
      const unhex = (value,size) => {
        if (typeof value !== 'string' || !new RegExp(`^[0-9a-f]{${size*2}}$`).test(value)) throw invalid();
        return Uint8Array.from(value.match(/../g),x=>parseInt(x,16));
      };
      const decode = (value,limit) => {
        if (typeof value !== 'string' || value.length > Math.ceil(limit/3)*4) throw invalid();
        const bytes = Uint8Array.from(atob(value),x=>x.charCodeAt(0));if(bytes.length>limit)throw invalid();return bytes;
      };
      if(bundle.format !== 'wpalt-attested-document-bundle-v1')throw invalid();
      const publicKey = unhex(bundle.public_key,32), signature = unhex(bundle.signature,64);
      if(hex(new Uint8Array(await crypto.subtle.digest('SHA-256',publicKey))) !== form.dataset.attestedDocument)throw invalid();
      const manifestBytes = decode(bundle.manifest,256*1024);
      const domain = new TextEncoder().encode('wpalt-response-document-attestation-v1\0');
      const message = new Uint8Array(domain.length+manifestBytes.length);message.set(domain);message.set(manifestBytes,domain.length);
      const key = await crypto.subtle.importKey('raw',publicKey,{name:'Ed25519'},false,['verify']);
      if(!await crypto.subtle.verify('Ed25519',key,signature,message))throw invalid();
      const manifest = JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(manifestBytes));
      const artifact = decode(bundle.artifact,8*1024*1024);
      if(manifest.source?.form_id !== form.dataset.documentForm || manifest.source?.entry_id !== form.dataset.documentEntry || manifest.media_type !== (body.get('kind') === 'pdf' ? 'application/pdf':'text/html') || manifest.format !== 'wpalt-response-document-v1' || !['text/html','application/pdf'].includes(manifest.media_type) || manifest.authority !== 'Server attestation only; respondent identity is unverified.' || !artifact.length || artifact.length !== manifest.artifact_bytes || hex(new Uint8Array(await crypto.subtle.digest('SHA-256',artifact))) !== manifest.artifact_sha256)throw invalid();
      const addDownload = (bytes,type,name,label) => {
        const link = document.createElement('a');link.href = URL.createObjectURL(new Blob([bytes],{type}));urls.push(link.href);
        link.download = name;link.textContent = label;downloads.append(link);
      };
      const pdf = manifest.media_type === 'application/pdf';
      addDownload(artifact,manifest.media_type,pdf?'response.pdf':'response.html',pdf?'Download PDF':'Download HTML');
      addDownload(raw,'application/json','response-document.json','Download verification bundle');
      downloads.hidden = false;status.textContent = 'Document checked against this site’s configured key. Keep both downloads. Respondent identity remains unverified.';
    } catch (error) {
      clear();status.className = 'notice error';status.setAttribute('role','alert');
      status.textContent = error.name === 'AbortError' ? 'Document preparation timed out. Your response and format choice remain here.' : (error.message || 'Document preparation failed. Your response remains here.');
    } finally {
      clearTimeout(timer);busy = false;button.disabled = false;format.disabled = false;form.removeAttribute('aria-busy');
    }
  });
}
