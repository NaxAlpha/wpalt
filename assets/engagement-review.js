(async()=>{
 const host=document.querySelector('#engagement-playback');if(!host)return;
 const element=(tag,text)=>{const node=document.createElement(tag);if(text)node.textContent=text;return node;};
 try{
  const response=await fetch(`/api/admin/engagement/sessions/${host.dataset.session}`,{credentials:'same-origin'});if(!response.ok)throw new Error('Could not load private frames.');const {frames}=await response.json();host.replaceChildren();
  if(!frames.length){host.append(element('p','No recorded geometry remains for this session.'));return;}
  const description=element('p');description.setAttribute('role','status');const controls=element('div');controls.className='toolbar';const canvas=element('canvas');canvas.className='engagement-wireframe';canvas.setAttribute('aria-label','Masked geometry and interaction heatmap');
  const slider=element('input');slider.type='range';slider.min='0';slider.max=String(frames.length-1);slider.value='0';slider.setAttribute('aria-label','Recorded frame');
  const heat=element('input');heat.type='checkbox';const label=element('label');label.append(heat,document.createTextNode('Show recorded click heatmap'));
  function draw(){const index=Number(slider.value);const record=frames[index],frame=record.geometry;canvas.width=frame.width;canvas.height=frame.height;const ctx=canvas.getContext('2d');ctx.fillStyle='#f7f5f0';ctx.fillRect(0,0,canvas.width,canvas.height);
   for(const rect of frame.rectangles){ctx.fillStyle=rect.kind==='control'?'#777d78':rect.kind==='image'?'#a0aaa0':'#d5d8d1';ctx.fillRect(rect.x,rect.y-frame.scroll_y,rect.width,rect.height);}
   const points=heat.checked?frames.filter(f=>f.path===record.path).map(f=>f.geometry.click).filter(Boolean):[frame.click].filter(Boolean);ctx.fillStyle='rgba(150,56,36,0.45)';for(const point of points){ctx.beginPath();ctx.arc(point[0],point[1]-frame.scroll_y,12,0,Math.PI*2);ctx.fill();}
   description.textContent=`Frame ${index+1} of ${frames.length} · ${record.path} · ${frame.elapsed}s`;
  }
  for(const [text,delta] of [['Previous frame',-1],['Next frame',1]]){const button=element('button',text);button.type='button';button.onclick=()=>{slider.value=String(Math.max(0,Math.min(frames.length-1,Number(slider.value)+delta)));draw();};controls.append(button);}
  slider.oninput=draw;heat.onchange=draw;host.append(description,controls,slider,label,canvas);draw();
 }catch(error){host.replaceChildren(element('p',error.message));}
})();
