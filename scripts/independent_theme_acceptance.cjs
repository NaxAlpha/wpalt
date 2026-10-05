const assert=require('node:assert/strict');const fs=require('node:fs');const path=require('node:path');const ui=require('./ui_contracts.cjs');
const contract=JSON.parse(fs.readFileSync(path.join(__dirname,'theme-contracts.json'),'utf8'));
async function measure(page){return page.evaluate(c=>{
 const failures=[],components=[];const near=(a,b)=>Math.abs(a-b)<=c.geometry_tolerance_px;const check=(v,message)=>{if(!v)failures.push(message);};
 check(document.documentElement.scrollWidth<=innerWidth+1,'Public theme horizontal overflow');
 const body=getComputedStyle(document.body);check(near(parseFloat(body.fontSize),c.body_font_px),'Public body typography drift');check(near(parseFloat(body.lineHeight),c.body_font_px*c.body_line_height),'Public line-height drift');
 const shell=getComputedStyle(document.querySelector('.theme-shell'));check(near(parseFloat(shell.paddingLeft),c.theme_shell_padding_px),'Public reading gutter drift');
 for(const el of document.querySelectorAll('input:not([type=hidden]),button,select')){if(!el.getClientRects().length)continue;const r=el.getBoundingClientRect(),s=getComputedStyle(el);components.push({kind:el.tagName,x:r.x,y:r.y,width:r.width,height:r.height,font:s.fontSize,radius:parseFloat(s.borderRadius)});check(r.height+c.geometry_tolerance_px>=c.control_min_height,'Public control target too short');check(near(parseFloat(s.fontSize),c.control_font_px),'Public control typography drift');if(!['checkbox','radio'].includes(el.type))check(near(parseFloat(s.borderRadius),c.control_radius),`Public control radius drift: ${el.tagName}.${el.className} (${s.borderRadius})`);else{const label=el.closest('label');check(!!label&&label.getBoundingClientRect().height+c.geometry_tolerance_px>=c.control_min_height,'Public choice label target too short');}check(r.x>=0&&r.right<=innerWidth+1,'Public control viewport overflow');}
 return {viewport:innerWidth,failures,components,body_font:body.fontSize,line_height:body.lineHeight};
},contract);}
module.exports=async(owner,publicContext,origin,output)=>{
 const admin=await owner.newPage();const page=await publicContext.newPage();const report={measurements:[],accessibility:[],journey:[]};let previous;
 await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
 await page.route(origin+'/__ui_fixture/theme-spacing.css',r=>r.fulfill({contentType:'text/css',body:'body *{line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}p{margin-bottom:2em!important}'}));
 try{
  await admin.goto(origin+'/admin');const csrf=await admin.locator('input[name=csrf]').first().inputValue();
  const state=await(await owner.request.get(origin+'/api/admin/design')).json();previous=state.active;
  const package=JSON.parse(fs.readFileSync(path.join(__dirname,'../examples/themes/field-journal.json'),'utf8'));
  for(const key of ['home','search']) package.templates[key].children[2].style={layout:'grid',columns:3,mobile_columns:1,gap:24};
  const save=(version,publish)=>owner.request.post(origin+'/api/admin/design/independent-journal',{headers:{Origin:origin},data:{csrf,version,package,publish}});
  let response=await save(0,false);assert.equal(response.status(),200,await response.text());
  response=await owner.request.post(origin+'/api/admin/design/independent-journal/activate',{headers:{Origin:origin},data:{csrf}});assert.notEqual(response.status(),200,'Unpublished third-party theme cannot activate');
  assert.notEqual((await publicContext.request.get(origin+'/admin/design/independent-journal/style.css',{maxRedirects:0})).status(),200,'Private theme styles require authority');
  response=await save(1,true);assert.equal(response.status(),200,await response.text());
  response=await owner.request.post(origin+'/api/admin/design/independent-journal/activate',{headers:{Origin:origin},data:{csrf}});assert.equal(response.status(),200,await response.text());
  for(const width of [320,768,1440]){
   await page.setViewportSize({width,height:1000});await page.goto(origin+'/');await page.locator('.n-homelist').waitFor();
   assert.equal(await page.locator('body').getAttribute('class'),'theme-site independent-journal');
   const layout=await page.locator('.n-homelist').evaluate(el=>{const s=getComputedStyle(el),r=el.getBoundingClientRect();return {columns:s.gridTemplateColumns.split(' ').length,gap:parseFloat(s.gap),width:r.width,x:r.x};});
   assert.equal(layout.columns,width<=700?1:3);assert.equal(layout.gap,24);
   let measured=await measure(page);assert.deepEqual(measured.failures,[]);report.measurements.push({surface:'home',...measured,layout});
   await page.screenshot({path:path.join(output,`independent-theme-home-${width}.png`),fullPage:true});
   await page.screenshot({path:path.join(output,`independent-theme-top-${width}.png`)});
   const spacing=await page.addStyleTag({url:origin+'/__ui_fixture/theme-spacing.css'});measured=await measure(page);assert.deepEqual(measured.failures,[]);report.measurements.push({surface:'home text spacing',...measured});await spacing.evaluate(n=>n.remove());
   const first=page.locator('.n-homelink').first();assert(await first.isVisible());assert(await page.locator('.n-homeexcerpt').first().textContent());const href=await first.getAttribute('href');await page.goto(origin+href);
   const article=await page.locator('.n-article').evaluate(el=>{const s=getComputedStyle(el),r=el.getBoundingClientRect();return {max_width:parseFloat(s.maxWidth),width:r.width,x:r.x};});
   assert.equal(article.max_width,760);assert(article.width<=760.5);assert(article.x>=0);assert(article.x+article.width<=width+.5);
   measured=await measure(page);assert.deepEqual(measured.failures,[]);report.measurements.push({surface:'content',...measured,article});
   await page.screenshot({path:path.join(output,`independent-theme-content-${width}.png`)});
  }
  await ui.accessibility(page,origin,'independent-theme-content',report);
  await page.goto(origin+'/');await ui.accessibility(page,origin,'independent-theme-home',report);
  await page.locator('.skip').focus();await page.keyboard.press('Enter');assert.equal(await page.evaluate(()=>document.activeElement.id),'main');
  report.status='passed';report.journey.push('Independent native package remains draft until deliberate owner publication, anonymous draft styles denied, populated grid has exact responsive columns/gap, reading width bounded, text spacing and keyboard/axe verified.');
 }finally{
  if(previous){const csrf=await admin.locator('input[name=csrf]').first().inputValue();await owner.request.post(origin+'/api/admin/design/'+previous+'/activate',{headers:{Origin:origin},data:{csrf}});}
  await admin.close();await page.close();fs.writeFileSync(path.join(output,'independent-theme-result.json'),JSON.stringify(report,null,2));
 }
};
