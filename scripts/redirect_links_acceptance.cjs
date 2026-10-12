// One connected native owner journey, with independent responsive/a11y contracts.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ui = require('./ui_contracts.cjs');
module.exports = async (owner, visitors, origin, output) => {
  const page=await owner.newPage();
  const report={status:'running',geometry:[],accessibility:[],errors:[],assertions:[]};
  page.on('pageerror',error=>report.errors.push(error.message));
  await page.route(origin+'/__ui_fixture/axe.js',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  const submit=async(p,button)=>Promise.all([p.waitForNavigation({waitUntil:'load'}),button.click()]);
  async function measure(name) {
    for(const width of [320,768,1440]) {
      await page.setViewportSize({width,height:1000});await page.evaluate(()=>scrollTo(0,0));
      const observation=await ui.geometry(page);assert.deepEqual(observation.failures,[]);report.geometry.push({surface:name,...observation});
      await page.screenshot({path:path.join(output,`d08-${name}-${width}.png`),fullPage:true});
      await page.screenshot({path:path.join(output,`d08-${name}-${width}-viewport.png`)});
    }
    await ui.accessibility(page,origin,name,report);
  }
  try {
    await page.goto(origin+'/admin/discovery/routes');
    const csrf=await page.locator('form[action="/admin/discovery/routes/settings"] input[name=csrf]').inputValue();
    const created=await owner.request.post(origin+'/api/admin/content',{headers:{Origin:origin,'X-CSRF-Token':csrf},data:{csrf,title:'Current route story',slug:'routes-current',kind:'post',fields:'{}',body:'A published destination for redirect verification.',action:'publish'}});
    assert(created.ok(),`Real publication returned ${created.status()}`);
    const publication=await created.json();
    const native=await owner.browser().newContext({javaScriptEnabled:false,storageState:await owner.storageState(),viewport:{width:320,height:1000}});
    try {
      const p=await native.newPage();await p.goto(origin+'/admin/discovery/routes');
      await p.getByLabel('Rule identifier',{exact:true}).fill('browser_move');
      await p.getByLabel('Label',{exact:true}).fill('Old story addresses');
      await p.getByLabel('Whole-path pattern',{exact:true}).fill('^/past/([a-z0-9-]+)$');
      await p.getByLabel('Local destination',{exact:true}).fill('/$1');
      await p.getByLabel('Sample path',{exact:true}).fill('/past/routes-current');
      await submit(p,p.getByRole('button',{name:'Preview rule',exact:true}));
      assert((await p.locator('section[aria-label="Rule preview"]').textContent()).includes('/past/routes-current → /routes-current'));
      const before=await(await owner.request.get(origin+'/api/discovery/redirect-policy')).json();assert.equal(before.policy.settings.rules.length,0,'Preview does not persist a rule');
      await submit(p,p.getByRole('button',{name:'Save rule',exact:true}));
      const moved=await visitors.request.get(origin+'/past/routes-current?secret=discarded',{maxRedirects:0});
      assert.equal(moved.status(),301);assert.equal(moved.headers().location,'/routes-current');
      await p.goto(origin+'/admin/discovery/routes?edit=browser_move');
      await p.getByLabel('Local destination',{exact:true}).fill('/account/$1');
      const [invalid]=await Promise.all([p.waitForResponse(r=>new URL(r.url()).pathname==='/admin/discovery/routes/rule'&&r.request().method()==='POST'),submit(p,p.getByRole('button',{name:'Save rule',exact:true}))]);
      assert.equal(invalid.status(),422);assert.equal(await p.getByLabel('Local destination',{exact:true}).inputValue(),'/account/$1');await p.getByRole('alert').waitFor();
      await p.goto(origin+'/admin/discovery/routes');
      await p.getByLabel('Retention · days',{exact:true}).fill('90');
      await p.getByLabel('Maximum retained paths',{exact:true}).fill('123');
      // A real concurrent policy edit makes the displayed settings version stale.
      const current=await(await owner.request.get(origin+'/api/discovery/redirect-policy')).json();
      const updated=await owner.request.post(origin+'/api/discovery/redirect-policy',{headers:{Origin:origin,'X-CSRF-Token':csrf},data:{csrf,version:current.version,settings:current.policy.settings}});assert(updated.ok());
      const [settingsConflict]=await Promise.all([p.waitForResponse(r=>new URL(r.url()).pathname==='/admin/discovery/routes/settings'&&r.request().method()==='POST'),submit(p,p.getByRole('button',{name:'Save route settings',exact:true}))]);
      assert.equal(settingsConflict.status(),409);assert.equal(await p.getByLabel('Retention · days',{exact:true}).inputValue(),'90');assert.equal(await p.getByLabel('Maximum retained paths',{exact:true}).inputValue(),'123');
      await p.getByLabel('Source path',{exact:true}).fill('/entered-native-source');
      await p.getByLabel('Destination path',{exact:true}).fill('https://outside.test/entered');
      const [exactInvalid]=await Promise.all([p.waitForResponse(r=>new URL(r.url()).pathname==='/admin/discovery/redirects'&&r.request().method()==='POST'),submit(p,p.getByRole('button',{name:'Add redirect',exact:true}))]);
      assert.equal(exactInvalid.status(),422);assert.equal(await p.getByLabel('Source path',{exact:true}).inputValue(),'/entered-native-source');assert.equal(await p.getByLabel('Destination path',{exact:true}).inputValue(),'https://outside.test/entered');
      await p.goto(origin+'/admin/discovery/routes');
      await p.getByLabel('Create redirects when a previously published address changes',{exact:true}).check();
      await p.getByLabel('Retain missing public paths',{exact:true}).check();
      await submit(p,p.getByRole('button',{name:'Save route settings',exact:true}));
      await p.goto(origin+'/admin/discovery/routes?edit=browser_move');
      const renamed=await owner.request.post(origin+'/api/admin/content/'+publication.id,{headers:{Origin:origin,'X-CSRF-Token':csrf},data:{csrf,title:'Current route story',slug:'routes-renamed',kind:'post',fields:'{}',body:'The actual later publication.',action:'publish',version:publication.version}});
      assert(renamed.ok(),`Publication rename returned ${renamed.status()}`);
      const [stale]=await Promise.all([p.waitForResponse(r=>new URL(r.url()).pathname==='/admin/discovery/routes/rule'&&r.request().method()==='POST'),submit(p,p.getByRole('button',{name:'Save rule',exact:true}))]);
      assert.equal(stale.status(),409);assert.equal(await p.getByLabel('Local destination',{exact:true}).inputValue(),'/$1');
      const chained=await visitors.request.get(origin+'/past/routes-current',{maxRedirects:0});assert.equal(chained.status(),301);assert.equal(chained.headers().location,'/routes-renamed');
      report.assertions.push('No-JavaScript preview/save; preview is read-only; invalid and stale rule/settings/exact forms retain inputs; actual publication aliases flatten the capture chain');
    } finally {await native.close();}
    for(const query of ['secret=discarded','other=also-discarded']) assert.equal((await visitors.request.get(origin+'/browser-missing?'+query)).status(),404);
    await page.goto(origin+'/admin/discovery/routes');
    const text=await page.locator('main').textContent();assert(text.includes('/browser-missing · 2 hits'));assert(!text.includes('secret=discarded'));
    await measure('route-workspace');
    await page.goto(origin+'/admin/discovery/rendered-links');
    await submit(page,page.getByRole('button',{name:'Analyze rendered publications',exact:true}));
    await page.getByRole('heading',{name:'Completed declared graph',exact:true}).waitFor();
    await measure('rendered-links');
    const downloaded=await owner.request.post(origin+'/admin/discovery/rendered-links',{headers:{Origin:origin},form:{csrf,output:'json'}});assert(downloaded.ok());assert(downloaded.headers()['content-disposition'].includes('wpalt-rendered-links.json'));
    const graph=await downloaded.json();assert.equal(graph.complete,true);assert(graph.sources.some(s=>s.path==='/routes-renamed'));assert(!graph.sources.some(s=>s.path==='/routes-current'));report.graph={sources:graph.sources.length,edges:graph.edges.length,html_bytes:graph.html_bytes,elapsed_ms:graph.elapsed_ms};
    report.assertions.push('Opt-in report aggregates paths without queries; rendered report and full download use the current anonymous publication graph');
    assert.deepEqual(report.errors,[]);report.status='passed';
  } catch(error) {report.errors.push(error.message);throw error;}
  finally {fs.writeFileSync(path.join(output,'d08-redirect-links.json'),JSON.stringify(report,null,2)+'\n');await page.close();}
};
