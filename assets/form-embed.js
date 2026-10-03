// Only same-origin form frames may request a bounded height; no payload capture.
(() => {
  if (window.wpaltFormFrames) return;
  window.wpaltFormFrames = true;
  if (window.parent !== window && document.body.classList.contains('embedded-form')) {
    const main=document.querySelector('main');
    const update=()=>parent.postMessage({kind:'wpalt:form-size',height:Math.ceil(main.getBoundingClientRect().height+32)},location.origin);
    new ResizeObserver(update).observe(main);update();return;
  }
  window.addEventListener('message',event=>{
    if(event.origin!==location.origin || event.data?.kind!=='wpalt:form-size' || !Number.isFinite(event.data.height))return;
    const frame=Array.from(document.querySelectorAll('.form-embed iframe')).find(frame=>frame.contentWindow===event.source);
    if(!frame)return;
    const url=new URL(frame.src);if(url.origin!==location.origin || !/^\/forms\/[a-f0-9-]{36}$/.test(url.pathname))return;
    frame.style.height=`${Math.max(160,Math.min(6000,event.data.height))}px`;
  });
})();
