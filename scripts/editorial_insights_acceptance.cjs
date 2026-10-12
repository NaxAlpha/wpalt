// Connected native editor/owner journey with precise responsive and scoped a11y checks.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ui=require('./ui_contracts.cjs');
module.exports=async(owner,visitors,origin,output)=>{
 const page=await owner.newPage(),report={status:'running',geometry:[],accessibility:[],errors:[],assertions:[],browser:owner.browser().version()};
 page.on('pageerror',e=>report.errors.push(e.message));
 await page.route(origin+'/__ui_fixture/axe.js',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
 const submit=async(p,button)=>Promise.all([p.waitForNavigation({waitUntil:'load'}),button.click()]);
 async function measure(name){for(const width of [320,768,1440]){await page.setViewportSize({width,height:1000});const m=await ui.geometry(page);assert.deepEqual(m.failures,[]);report.geometry.push({surface:name,...m});await page.screenshot({path:path.join(output,`d09-${name}-${width}.png`),fullPage:true});await page.screenshot({path:path.join(output,`d09-${name}-${width}-viewport.png`)});}await ui.accessibility(page,origin,name,report);}
 try{
  await page.goto(origin+'/admin/posts/new');const csrf=await page.locator('input[name=csrf]').first().inputValue();
  const save=async(id,value)=>{const r=await owner.request.post(origin+'/api/admin/content'+(id?'/'+id:''),{headers:{Origin:origin,'X-CSRF-Token':csrf},data:{csrf,kind:'post',fields:'{}',...value}});assert(r.ok(),`Content operation ${r.status()}`);return r.json();};
  const publicCopy=await save(null,{title:'Café editorial guide',slug:'editorial-actual-copy',body:'# Café\n\nCafé for actual readers. [Missing resource](/editorial-missing)',action:'publish'});
  const workingCopy=await save(publicCopy.id,{title:'Unfinished editor draft',slug:'editorial-actual-copy',body:'# Unfinished\n\nPRIVATE WORKING editorial text.',action:'save',version:publicCopy.version});
  const native=await owner.browser().newContext({javaScriptEnabled:false,storageState:await owner.storageState(),viewport:{width:320,height:1000}});
  try{
   const p=await native.newPage();await p.goto(origin+'/admin/posts/'+publicCopy.id);await p.getByRole('link',{name:'Editorial insights',exact:true}).click();
   await p.getByLabel('Focus phrase (optional)',{exact:true}).fill('PRIVATE WORKING');await submit(p,p.getByRole('button',{name:'Analyze selected copy',exact:true}));
   assert((await p.locator('main').textContent()).includes('PRIVATE WORKING editorial text.'));
   await p.getByLabel('Source copy',{exact:true}).selectOption('public');await p.getByLabel('Focus phrase (optional)',{exact:true}).fill('CAFÉ');await submit(p,p.getByRole('button',{name:'Analyze selected copy',exact:true}));
   assert((await p.locator('main').textContent()).includes('Café for actual readers.'));assert(!(await p.locator('main').textContent()).includes('PRIVATE WORKING editorial text.'));
   await p.getByLabel('Focus phrase (optional)',{exact:true}).fill('one two three four five six seven eight nine');
   const [invalid]=await Promise.all([p.waitForResponse(r=>r.request().method()==='POST'&&r.url().endsWith('/admin/insights/'+publicCopy.id)),submit(p,p.getByRole('button',{name:'Analyze selected copy',exact:true}))]);
   assert.equal(invalid.status(),422);assert.equal(await p.getByLabel('Source copy',{exact:true}).inputValue(),'public');assert.equal(await p.getByLabel('Focus phrase (optional)',{exact:true}).inputValue(),'one two three four five six seven eight nine');await p.getByRole('alert').waitFor();
   report.assertions.push('No-JavaScript source selection analyzes the saved working copy or actual anonymous publication; invalid forms preserve copy and phrase');
  }finally{await native.close();}
  await page.goto(origin+'/admin/insights/'+publicCopy.id);await page.getByLabel('Source copy',{exact:true}).selectOption('public');await page.getByLabel('Focus phrase (optional)',{exact:true}).fill('café');await submit(page,page.getByRole('button',{name:'Analyze selected copy',exact:true}));await measure('editor-observations');
  await page.getByLabel('Focus phrase (optional)',{exact:true}).fill('one two three four five six seven eight nine');await submit(page,page.getByRole('button',{name:'Analyze selected copy',exact:true}));await measure('editor-error');
  await page.goto(origin+'/admin/analysis');await page.getByLabel('Focus phrase (optional)',{exact:true}).fill('café');await submit(page,page.getByRole('button',{name:'Start public analysis',exact:true}));
  let runs=await(await owner.request.get(origin+'/api/admin/analysis')).json();const id=runs[0].id;
  const incomplete=await owner.request.get(origin+'/api/admin/analysis/'+id+'/sources');assert.equal(incomplete.status(),422);
  for(let i=0;i<20;i++){runs=await(await owner.request.get(origin+'/api/admin/analysis')).json();const run=runs.find(r=>r.id===id);if(run.state==='complete')break;assert(['rendering','classifying'].includes(run.state));const r=await owner.request.post(origin+'/api/admin/analysis/'+id,{headers:{Origin:origin},data:{csrf,version:run.version,action:'advance'}});assert(r.ok());}
  runs=await(await owner.request.get(origin+'/api/admin/analysis')).json();const run=runs.find(r=>r.id===id);assert.equal(run.state,'complete');report.run={sources:run.total,edges:run.edge_count,html_bytes:run.html_bytes,facts_bytes:run.facts_bytes};
  await page.goto(origin+'/admin/analysis');await measure('completed-runs');
  const sources=await(await owner.request.get(origin+'/api/admin/analysis/'+id+'/sources')).json();const source=sources.find(s=>s.publication===publicCopy.id);assert(source);
  await page.goto(origin+'/admin/analysis/'+id+'/sources/'+source.ordinal);assert((await page.locator('main').textContent()).includes('/editorial-missing'));await measure('source-details');
  if(process.env.WPALT_EDITORIAL_REFERENCE) await require('./editorial_reference.cjs')(owner,visitors,origin,output,publicCopy.id,csrf);
  await save(publicCopy.id,{title:'Changed factual publication',slug:'editorial-actual-copy',body:'The later public source.',action:'publish',version:workingCopy.version});
  const stale=await owner.request.get(origin+'/api/admin/analysis/'+id+'/sources/'+source.ordinal);assert.equal(stale.status(),422);assert(!(await stale.text()).includes('Café for actual readers.'));
  await page.goto(origin+'/admin/analysis');await measure('stale-runs');
  assert(!(await page.locator('main').textContent()).includes('Focus phrase: café'));
  report.assertions.push('Partial graphs refuse conclusions; complete sources link back to shared editing; actual publication invalidates retained details and suppresses focus text');
  const cancel=await owner.request.post(origin+'/api/admin/analysis',{headers:{Origin:origin},data:{csrf,focus:''}});assert(cancel.ok());const cancelId=(await cancel.json()).id;
  runs=await(await owner.request.get(origin+'/api/admin/analysis')).json();const active=runs.find(r=>r.id===cancelId);
  const cancelled=await owner.request.post(origin+'/api/admin/analysis/'+cancelId,{headers:{Origin:origin},data:{csrf,version:active.version,action:'cancel'}});assert(cancelled.ok());
  const bad=await owner.request.post(origin+'/api/admin/analysis',{headers:{Origin:origin},data:{csrf:'wrong',focus:''}});assert.equal(bad.status(),403);
  assert.equal((await visitors.request.get(origin+'/api/admin/analysis',{maxRedirects:0})).status(),401);
  assert.deepEqual(report.errors,[]);report.status='passed';
 }catch(e){report.errors.push(e.message);throw e;}finally{fs.writeFileSync(path.join(output,'d09-editorial-insights.json'),JSON.stringify(report,null,2)+'\n');await page.close();}
};
