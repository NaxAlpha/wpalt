// A connected real-browser owner import, native edit and explicit publication.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ui=require('./ui_contracts.cjs');
module.exports=async(owner,visitors,origin,output)=>{
 const admin=await owner.newPage(),page=await visitors.newPage();
 const report={status:'incomplete',browser:visitors.browser().version(),measurements:[],accessibility:[],errors:[],observations:[]};
 admin.on('pageerror',e=>report.errors.push(e.message));let previous,csrf,original;
 const source={version:'0.4',title:'Browser design',type:'page',page_settings:[],content:[{id:'root',elType:'container',settings:{flex_direction:'column',gap:{unit:'px',size:24}},elements:[{id:'heading',elType:'widget',widgetType:'heading',settings:{title:'BROWSER_IMPORTED_HERO',header_size:'h1',title_color:'#24594f',typography_font_size:{unit:'px',size:32},typography_font_family:'Source font',custom_css:'never execute()'},elements:[]},{id:'copy',elType:'widget',widgetType:'text-editor',settings:{editor:'<p>A calm <strong>local</strong> design.</p><script>window.__unsafeImport=true</script>'},elements:[]},{id:'button',elType:'widget',widgetType:'button',settings:{text:'Explore local publishing',link:{url:'/search'}},elements:[]}]}]};
 async function state(){const r=await owner.request.get(origin+'/api/admin/design');assert(r.ok());return r.json();}
 try{
  await admin.goto(origin+'/admin/builder');await admin.getByText('Review an Elementor design import',{exact:true}).waitFor();
  let current=await state();previous=current.active;csrf=await admin.locator('#builder').getAttribute('data-csrf');original=current.themes.find(t=>t.id===previous).package;
  await admin.getByText('Review an Elementor design import',{exact:true}).click();
  const file=admin.locator('label.file-button').filter({hasText:'Choose Elementor JSON export'}).locator('input');
  await file.setInputFiles({name:'design.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(source))});
  await admin.getByLabel('New reusable component name',{exact:true}).fill('browser-import');
  await admin.getByLabel('Import placement',{exact:true}).selectOption('home');
  await admin.getByLabel('Local font for Source font',{exact:true}).selectOption('serif');
  await admin.getByRole('button',{name:'Review native mapping',exact:true}).click();
  await admin.getByRole('region',{name:'Elementor import review',exact:true}).waitFor();
  assert(await admin.getByText(/settings not mapped/).count());
  for(const width of [320,768,1440]){
   await admin.setViewportSize({width,height:1000});const geometry=await ui.geometry(admin);assert.deepEqual(geometry.failures,[]);report.measurements.push({surface:'reviewed import',...geometry});
   await admin.screenshot({path:path.join(output,`d05-import-review-${width}.png`),fullPage:true});
   await admin.getByRole('region',{name:'Elementor import review',exact:true}).scrollIntoViewIfNeeded();
   await admin.screenshot({path:path.join(output,`d05-import-review-viewport-${width}.png`)});
  }
  await admin.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  await ui.accessibility(admin,origin,'reviewed Elementor import',report);
  assert.deepEqual(report.accessibility.at(-1).incomplete,[],'New import surface has no unresolved automated findings');
  await admin.getByLabel('I reviewed the mapping and every reported loss.').check();
  await admin.getByRole('button',{name:'Import reviewed private draft',exact:true}).click();
  await admin.getByLabel('Template',{exact:true}).locator('option').filter({hasText:'browser-import'}).waitFor({state:'attached'});
  await page.goto(origin+'/');assert.equal(await page.getByText('BROWSER_IMPORTED_HERO',{exact:true}).count(),0);
  await page.goto(origin+'/admin/design/'+previous+'/preview?template=home');assert.equal(await page.getByText('BROWSER_IMPORTED_HERO',{exact:true}).count(),0,'Visitors cannot inspect private previews');
  const preview=await owner.request.get(origin+'/admin/design/'+previous+'/preview?template=home');assert(preview.ok());assert((await preview.text()).includes('BROWSER_IMPORTED_HERO'));
  current=await state();const theme=current.themes.find(t=>t.id===previous);assert.equal(theme.package.components['browser-import'].root.children[0].children[0].text,'BROWSER_IMPORTED_HERO');
  theme.package.components['browser-import'].root.children[0].children[0].text='BROWSER_EDITED_NATIVE_HERO';
  const published=await owner.request.post(origin+'/api/admin/design/'+previous,{headers:{Origin:origin},data:{csrf,version:theme.version,package:theme.package,publish:true}});assert(published.ok(),await published.text());
  for(const width of [320,768,1440]){
   await page.setViewportSize({width,height:1000});await page.goto(origin+'/');await page.getByRole('heading',{name:'BROWSER_EDITED_NATIVE_HERO',exact:true}).waitFor();assert.equal(await page.evaluate(()=>window.__unsafeImport),undefined);
   const observed=await page.getByRole('heading',{name:'BROWSER_EDITED_NATIVE_HERO',exact:true}).evaluate(el=>{const r=el.getBoundingClientRect(),s=getComputedStyle(el);return{viewport:innerWidth,width:r.width,font_size:s.fontSize,color:s.color,overflow:document.documentElement.scrollWidth>innerWidth+1}});assert.equal(observed.font_size,'32px');assert(!observed.overflow);report.observations.push(observed);
   await page.screenshot({path:path.join(output,`d05-native-public-${width}.png`),fullPage:true});
  }
  assert.deepEqual(report.errors,[]);report.status='passed';
 }finally{
  if(original&&csrf){const current=await state();const theme=current.themes.find(t=>t.id===previous);const restored=await owner.request.post(origin+'/api/admin/design/'+previous,{headers:{Origin:origin},data:{csrf,version:theme.version,package:original,publish:true}});assert(restored.ok(),await restored.text());}
  fs.writeFileSync(path.join(output,'d05-elementor-design.json'),JSON.stringify(report,null,2)+'\n');await admin.close();await page.close();
 }
};
