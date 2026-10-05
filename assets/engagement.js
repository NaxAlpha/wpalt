// First-party consent and geometry only. Never read field values or DOM text.
(async () => {
  const host = document.querySelector('#engagement-controls');
  if (!host) return;
  const element = (tag, text) => { const node = document.createElement(tag); if(text)node.textContent=text; return node; };
  const send = async (path, body) => {
    const response = await fetch(path,{method:body?'POST':'GET',credentials:'same-origin',headers:body?{'Content-Type':'application/json'}:{},body:body?JSON.stringify(body):undefined});
    if(!response.ok) throw new Error('Privacy request could not be completed.');return response.json();
  };
  const signal = navigator.globalPrivacyControl === true || (navigator.doNotTrack === '1' && host.dataset.respectDnt === 'true');
  let active=false,recording=false,frames=0,started=0,timer,privacyGeneration=0;
  const loadedScripts=new Set();
  const channel=typeof BroadcastChannel==='function'?new BroadcastChannel('wpalt:privacy'):null;
  if(channel)channel.onmessage=()=>{if(loadedScripts.size)location.reload();else stop();};
  async function loadScripts(state){
    if(!active || signal || !state.manifest)return;
    for(const declaration of state.scripts || []){
      const id=declaration.id;
      if(loadedScripts.has(id))continue;
      // The server independently verifies the current grant before returning any bytes.
      const script=element('script');script.src=`/api/engagement/scripts/${state.manifest}/${encodeURIComponent(id)}`;
      script.addEventListener('error',()=>{host.append(element('p',`Optional script ${declaration.label} could not be loaded. Review privacy choices or contact the site owner.`));},{once:true});
      loadedScripts.add(id);document.head.append(script);
    }
  }
  const dimensions={device:innerWidth<768?'mobile':'desktop',referrer:document.referrer?(new URL(document.referrer).origin===location.origin?'same_site':'external'):'direct'};
  const track=async (name,extra={})=>{if(!active || signal)return;try{await send('/api/engagement/events',{id:crypto.randomUUID(),path:location.pathname,name,dimensions,...extra});}catch{}};
  const snapshot=click=>{
    if(!active || !recording || frames>=60 || performance.now()-started>900000)return;
    frames++;
    const quantize=value=>Math.max(0,Math.round(value/8)*8);
    const rectangles=Array.from(document.querySelectorAll('main h1,main h2,main p,main img,main a,main button,main input,main textarea,main select')).slice(0,200).map(node=>{
      const rect=node.getBoundingClientRect();return {x:quantize(rect.x),y:quantize(rect.y+scrollY),width:quantize(Math.min(rect.width,4096)),height:quantize(Math.min(rect.height,100000)),kind:node.tagName==='IMG'?'image':['INPUT','TEXTAREA','SELECT','BUTTON','A'].includes(node.tagName)?'control':'block'};
    });
    track('interaction',{frame:{width:Math.min(innerWidth,4096),height:Math.min(innerHeight,4096),scroll_y:quantize(scrollY),elapsed:Math.floor((performance.now()-started)/1000),rectangles,click:click?[quantize(click.clientX),quantize(click.clientY+scrollY)]:null}});
  };
  function stop(){privacyGeneration++;active=false;recording=false;clearInterval(timer);document.querySelector('#local-offer')?.remove();}
  function start(state){stop();active=state.consented;recording=state.recording;started=performance.now();if(active){loadScripts(state);track('pageview');offer();if(recording){snapshot();timer=setInterval(()=>snapshot(),15000);}}}
  async function offer(){
    const generation=privacyGeneration;
    try {
      const result=await send('/api/engagement/offers',{path:location.pathname,...dimensions});
      if(!active || generation!==privacyGeneration || !result.offer)return;
      const data=result.offer,dialog=element('dialog');dialog.id='local-offer';
      const heading=element('h2',data.title);heading.id='local-offer-title';dialog.setAttribute('aria-labelledby',heading.id);dialog.append(heading);
      const message=element('div');message.className='prose';message.innerHTML=data.html;dialog.append(message);
      const status=element('p');status.setAttribute('role','status');
      if(data.wheel && data.rewards.length){
        dialog.append(element('p','One weighted local draw per consented visitor session. Available rewards:'));
        const list=element('ul');for(const reward of data.rewards)list.append(element('li',reward.label));dialog.append(list);
        const draw=element('button','Draw a local reward');draw.type='button';draw.onclick=async()=>{draw.disabled=true;try{const receipt=await send(`/api/engagement/offers/${data.id}/claim`,{});status.textContent=`${receipt.label} — ${receipt.code}. Keep this receipt. Redemption is arranged with the site owner.`;}catch{status.textContent='The draw could not be completed. Retry to recover your receipt safely.';draw.disabled=false;}};dialog.append(draw);
      }
      const close=element('button','Close offer');close.type='button';close.onclick=()=>dialog.close();dialog.append(status,close);
      const previous=document.activeElement;dialog.addEventListener('close',()=>{dialog.remove();if(previous?.isConnected)previous.focus();},{once:true});
      document.body.append(dialog);dialog.showModal();close.focus();
    }catch{/* No popup when targeting cannot be verified. */}
  }
  function choices(state){
    host.replaceChildren();host.append(element('h2','Your privacy choices'),element('p',state.purpose));
    if(state.scripts?.length){host.append(element('p','Optional owner-hosted scripts included in this analytics choice:'));const list=element('ul');for(const script of state.scripts)list.append(element('li',`${script.label}: ${script.purpose}`));host.append(list);}
    const status=element('p');status.setAttribute('role','status');
    const recordingChoice=element('input');recordingChoice.type='checkbox';
    if(state.recording_available){const label=element('label');label.append(recordingChoice,document.createTextNode('Also allow optional masked interaction recording'));host.append(label);}
    const controls=element('div');controls.className='toolbar';
    const button=(text,action)=>{const button=element('button',text);button.type='button';button.addEventListener('click',async()=>{button.disabled=true;try{await action();}catch(error){status.textContent=error.message;}finally{button.disabled=false;}});controls.append(button);};
    if(!state.consented)button(state.scripts?.length?'Allow local analytics and declared scripts':'Allow local analytics',async()=>{await send('/api/engagement/consent',{allow:true,recording:recordingChoice.checked,policy:state.policy,manifest:state.manifest});const current=await send('/api/engagement/status');try{localStorage.removeItem('wpalt:analytics-declined');}catch{}start(current);choices(current);});
    button(state.consented?'Withdraw and erase my analytics':'Decline analytics',async()=>{await send('/api/engagement/consent',{allow:false,recording:false,policy:state.policy});stop();channel?.postMessage('withdrawn');if(loadedScripts.size){location.reload();return;}host.replaceChildren(element('p','Analytics declined. Recorded data for this visitor session has been removed.'));const reset=element('button','Review privacy choices');reset.type='button';reset.onclick=()=>choices({...state,consented:false});host.append(reset);try{localStorage.setItem('wpalt:analytics-declined',String(state.policy));}catch{}});
    host.append(controls,status);
  }
  try {
    if(signal){await send('/api/engagement/consent',{allow:false,recording:false,policy:Number(host.dataset.policy)});host.append(element('p','Analytics are off because of your browser privacy preference.'));return;}
    const state=await send('/api/engagement/status');if(!state.enabled){host.remove();return;}
    start(state);
    let declined=false;try{declined=localStorage.getItem('wpalt:analytics-declined')===String(state.policy);}catch{}
    if(!state.consented && declined){host.replaceChildren(element('p','Local analytics are declined.'));const review=element('button','Review privacy choices');review.type='button';review.onclick=()=>choices(state);host.append(review);}else choices(state);
    window.wpaltAnalytics={track};
    document.addEventListener('click',event=>{if(recording && event.isTrusted)snapshot(event);},{passive:true});
    window.addEventListener('pagehide',stop,{once:true});
  }catch{host.append(element('p','Analytics remain off until privacy choices can be verified.'));stop();}
})();
