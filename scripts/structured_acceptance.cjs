// An owner builds a location model, publishes through typed controls and a
// no-JavaScript form, and checks the same facts in visible output and JSON-LD.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ui = require('./ui_contracts.cjs');
module.exports = async (owner, visitors, origin, output) => {
  const page = await owner.newPage(), visitor = await visitors.newPage();let secondLocation;
  const report = {status:'running',geometry:[],accessibility:[],vocabulary_requests:0,errors:[]};
  page.on('pageerror', e => report.errors.push(e.message));
  page.on('request', r => {if(new URL(r.url()).pathname==='/api/admin/structured/vocabulary') report.vocabulary_requests++;});
  await page.route(origin+'/__ui_fixture/axe.js',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  const submit = async (p,button) => Promise.all([p.waitForNavigation({waitUntil:'load'}),button.click()]);
  function graph(source) {const values=JSON.parse(source);return values.find(v=>v['@graph']);}
  try {
    await page.goto(origin+'/admin/builder');await page.getByRole('button',{name:'models',exact:true}).click();
    await page.getByLabel('Add model identifier',{exact:true}).fill('browser_location');
    await page.getByRole('button',{name:'Add model',exact:true}).click();
    await page.getByLabel('Model label',{exact:true}).fill('Business locations');
    await page.getByText('Published structured facts',{exact:true}).click();
    await page.getByLabel('Structured starting definition',{exact:true}).selectOption('location');
    await page.getByRole('button',{name:'Create structured definition',exact:true}).click();
    assert.equal(report.vocabulary_requests,0,'The builder does not load every term or search hidden controls');
    await page.getByText('Add type to location',{exact:true}).click();
    await page.getByLabel('Add type to location prefix',{exact:true}).fill('Restaurant');
    await page.getByRole('button',{name:'Search type terms',exact:true}).click();
    await page.getByRole('button',{name:'Restaurant',exact:true}).click();
    await page.getByRole('button',{name:'Remove LocalBusiness',exact:true}).click();
    assert.equal(report.vocabulary_requests,1);
    for(const width of [320,768,1440]) {
      await page.setViewportSize({width,height:1000});
      const measured=await ui.geometry(page);assert.deepEqual(measured.failures,[]);report.geometry.push({surface:'Location graph builder',...measured});
      await page.screenshot({path:path.join(output,`d07-structured-studio-${width}.png`),fullPage:true});
    }
    await ui.accessibility(page,origin,'Location graph builder',report);
    await page.getByRole('button',{name:'Save model',exact:true}).click();
    await page.getByText('Model saved after existing-data validation',{exact:true}).waitFor();
    await page.goto(origin+'/admin/posts/new');
    await page.getByText('Typed fields & composition',{exact:true}).click();
    await page.getByLabel('Content type',{exact:true}).selectOption('browser_location');
    await page.getByLabel('Public name',{exact:true}).fill('Downtown café');
    await page.getByLabel('Street',{exact:true}).fill('1 Main St');
    await page.getByLabel('City',{exact:true}).fill('Tokyo');
    await page.getByLabel('Country',{exact:true}).fill('Japan');
    await page.getByLabel('Public telephone',{exact:true}).fill('+81 03 5555 0101');
    const hours=page.getByRole('group',{name:'Opening hours',exact:true});
    await hours.getByRole('button',{name:'Add row',exact:true}).click();
    await hours.getByLabel('Add Days',{exact:true}).selectOption('monday');
    await hours.getByLabel('Opens (HH:MM)',{exact:true}).fill('09:30');
    await hours.getByLabel('Closes (HH:MM)',{exact:true}).fill('18:00');
    await page.getByLabel('Title',{exact:true}).fill('Visit our downtown café');
    await page.getByText('Language & discovery',{exact:true}).click();
    await page.locator('[name=slug]').fill('structured-downtown');
    await page.locator('.ProseMirror').fill('The actual downtown location, with public contact and opening hours.');
    await submit(page,page.getByRole('button',{name:'Publish now',exact:true}));
    const editor=page.url();
    await visitor.goto(origin+'/structured-downtown');
    await visitor.getByRole('heading',{name:'Published facts',exact:true}).waitFor();
    const published=graph(await visitor.locator('script[type="application/ld+json"]').textContent());
    const nodes=published['@graph'];assert.equal(nodes[0]['@type'],'Restaurant');
    const facts=await visitor.locator('[data-structured-id]').evaluateAll(elements=>elements.map(el=>({id:el.dataset.structuredId,heading:el.querySelector('h3').textContent,text:el.textContent})));
    assert.equal(facts.length,nodes.length);assert.equal(await visitor.locator('.structured-facts').count(),1);
    assert(facts.some(f=>f.text.includes('1 Main St'))&&facts.some(f=>f.text.includes('09:30')));report.visible_facts=facts;
    fs.writeFileSync(path.join(output,'d07-structured-public.html'),await visitor.content());
    assert(nodes.some(n=>n.streetAddress==='1 Main St'));assert(nodes.some(n=>n.opens==='09:30'&&n.dayOfWeek==='https://schema.org/Monday'));
    assert((await visitor.locator('.structured-facts').textContent()).includes('1 Main St'));
    for(const width of [320,768,1440]) {
      await visitor.setViewportSize({width,height:1000});assert(await visitor.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'Published facts must reflow');
      await visitor.screenshot({path:path.join(output,`d07-structured-public-${width}.png`),fullPage:true});
    }
    await visitor.route(origin+'/__ui_fixture/axe.js',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
    await ui.accessibility(visitor,origin,'Published location facts',report);
    // Native fallback creates a second custom-model record without JavaScript.
    const native=await owner.browser().newContext({javaScriptEnabled:false,storageState:await owner.storageState(),viewport:{width:320,height:1000}});
    try {
      const p=await native.newPage();await p.goto(origin+'/admin/posts/new');
      await p.getByText('Typed fields & composition',{exact:true}).click();
      await p.getByLabel('Content type',{exact:true}).selectOption('browser_location');
      await p.locator('[name=fields]').fill(JSON.stringify({public_name:'Garden café',address:{street:'2 Garden St',city:'Kyoto',country:'Japan'}}));
      await p.getByLabel('Title',{exact:true}).fill('Visit the garden café');
      await p.getByLabel('Content',{exact:true}).fill('Native authoring for a second location.');
      await p.getByText('Language & discovery',{exact:true}).click();await p.locator('[name=slug]').fill('structured-garden');
      await submit(p,p.getByRole('button',{name:'Publish now',exact:true}));
      assert(p.url().includes('/admin/posts/'));secondLocation=p.url().split('/').pop();
      const r=await visitors.request.get(origin+'/structured-garden');assert(r.ok());assert((await r.text()).includes('2 Garden St'));
      await p.screenshot({path:path.join(output,'d07-native-location-editor-320.png'),fullPage:true});
    } finally {await native.close();}
    const builderHtml=await(await owner.request.get(origin+'/admin/builder')).text();const csrf=/data-csrf="([^"]+)"/.exec(builderHtml)[1];
    async function api(path,body){const response=await owner.request.post(origin+path,{headers:{Origin:origin,'X-CSRF-Token':csrf},data:{csrf,...body}});assert(response.ok(),`Structured fixture request ${path} returned ${response.status()}`);return response.json();}
    const orgDefinition={label:'Location organization',fields:{name:{kind:'string'},locations:{kind:'relationships',target:'browser_location'}},structured:{vocabulary:'30.1',primary:'organization',entities:{organization:{types:['Organization'],properties:{name:{kind:'field',path:['name'],datatype:'Text'},location:{kind:'relationship',path:['locations'],entity:'location'}}}}}};
    await api('/api/admin/models/browser_org',{version:0,definition:orgDefinition});
    await api('/api/admin/content',{title:'Our locations',slug:'structured-organization',kind:'browser_org',fields:JSON.stringify({name:'Independent cafés',locations:[editor.split('/').pop(),secondLocation]}),body:'Visit our two independently published locations.',action:'publish'});
    await visitor.goto(origin+'/structured-organization');
    const anchors=await visitor.locator('.structured-facts').evaluate(section=>{const ids=[...section.querySelectorAll('[data-structured-id]')].map(el=>el.id);return {ids,links:[...section.querySelectorAll('a[href^="#"]')].map(a=>({text:a.textContent,target:a.getAttribute('href').slice(1),matches:section.querySelectorAll(`[id="${a.getAttribute('href').slice(1)}"]`).length}))};});
    assert.equal(new Set(anchors.ids).size,anchors.ids.length,'Multiple locations must not share visible anchor IDs');
    assert(anchors.links.length>=2&&anchors.links.every(a=>a.matches===1),'Every related-entity link resolves to one visible facts section on this page');report.multiple_location_anchors=anchors;
    await visitor.setViewportSize({width:1440,height:1000});await visitor.screenshot({path:path.join(output,'d07-multiple-locations-1440.png'),fullPage:true});
    await page.goto(editor);await page.getByText('Typed fields & composition',{exact:true}).click();
    await page.getByLabel('Public name',{exact:true}).fill('PRIVATE_WORKING_LOCATION');
    await submit(page,page.getByRole('button',{name:'Save draft',exact:true}));
    assert(!(await (await visitors.request.get(origin+'/structured-downtown')).text()).includes('PRIVATE_WORKING_LOCATION'));
    const preview=await owner.request.get(origin+await page.getByRole('link',{name:'Preview',exact:true}).getAttribute('href'));
    const previewBody=await preview.text();assert(previewBody.includes('PRIVATE_WORKING_LOCATION')&&previewBody.includes('Structured output inspection'));assert(!previewBody.includes('application/ld+json'));
    assert.deepEqual(report.errors,[]);report.status='passed';report.public_graph=published;
  } finally {fs.writeFileSync(path.join(output,'d07-structured.json'),JSON.stringify(report,null,2)+'\n');await page.close();await visitor.close();}
};
