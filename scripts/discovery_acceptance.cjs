// Connected owner discovery edits, publication and current-asset revocation.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ui=require('./ui_contracts.cjs');
module.exports=async(owner,visitors,origin,output)=>{
 const page=await owner.newPage(),visitor=await visitors.newPage();
 const report={status:'incomplete',browser:visitors.browser().version(),measurements:[],accessibility:[],errors:[],observations:[]};
 page.on('pageerror',e=>report.errors.push(e.message));
 const submit=async(button)=>{const [response]=await Promise.all([page.waitForResponse(r=>r.request().method()==='POST'&&new URL(r.url()).pathname.startsWith('/admin/posts/')),page.waitForNavigation({waitUntil:'load'}),button.click()]);if(response.status()>=400)throw new Error(await response.text());};
 try{
  await page.goto(origin+'/admin/posts/new');let csrf=await page.locator('#main [name=csrf]').first().inputValue();
  const uploaded=await owner.request.post(origin+'/admin/media',{headers:{Origin:origin},multipart:{csrf,visibility:'public',alt:'A green garden',file:{name:'d06-garden.png',mimeType:'image/png',buffer:fs.readFileSync(path.join(__dirname,'../tests/fixtures/green.png'))}}});assert(uploaded.ok(),await uploaded.text());
  const images=await (await owner.request.get(origin+'/api/admin/media')).json();const image=images.items.find(i=>i.name==='d06-garden.png');assert(image);
  const processed=await owner.request.post(origin+'/admin/media/video',{headers:{Origin:origin},multipart:{csrf,visibility:'public',file:{name:'d06-film.mp4',mimeType:'video/mp4',buffer:fs.readFileSync(process.env.WPALT_VIDEO_FIXTURE)}}});assert(processed.ok(),await processed.text());
  const videos=await (await owner.request.get(origin+'/api/admin/media?media_kind=video')).json();const video=videos.items.find(i=>i.name==='d06-film.mp4');assert(video);
  const created=await owner.request.post(origin+'/api/admin/content',{headers:{Origin:origin,'X-CSRF-Token':csrf},data:{csrf,title:'A discovery journal',slug:'browser-discovery-journal',kind:'post',body:`A central relevant film and an editorial image. ![Garden](/media/${image.id})`,fields:'{}',blocks:'[]',action:'save'}});assert(created.ok(),await created.text());const post=await created.json();const editorUrl=origin+'/admin/posts/'+post.id;
  await page.goto(editorUrl);await page.getByText('Language & discovery',{exact:true}).click();
  await page.locator('[name=seo_title]').fill('A calm discovery title');await page.locator('[name=seo_description]').fill('A truthful description, with no promise of outside appearance.');await page.locator('[name=seo_type]').selectOption('Article');
  for(const label of ['Social image','Attached local video','News publication'])await page.locator('summary').filter({hasText:new RegExp('^'+label+'$')}).click();
  // Choose actual library assets through the same bounded native picker.
  for(const [field,name]of [['seo_social_image','d06-garden.png'],['seo_video_media','d06-film.mp4'],['seo_video_poster','d06-garden.png']]){
   const control=page.locator(`[data-discovery-media=${field}]`);await control.getByRole('button',{name:'Choose from library',exact:true}).click();await control.getByRole('button',{name:name+' · public',exact:true}).click();
  }
  await page.locator('[name=seo_social_alt]').fill('A garden image');await page.locator('[name=seo_video_title]').fill('The local garden film');await page.locator('[name=seo_video_description]').fill('A film that is central to this article.');await page.locator('[name=seo_video_sitemap]').check();await page.locator('[name=seo_news_publication]').fill('Local Journal');await page.locator('[name=seo_news_language]').fill('en');
  await page.locator('[data-preview-title]').getByText('A calm discovery title',{exact:true}).waitFor();
  await page.locator('[data-preview-image]').evaluate(img=>img.decode());
  await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  for(const width of [320,768,1440]){
   await page.setViewportSize({width,height:1000});const geometry=await ui.geometry(page);assert.deepEqual(geometry.failures,[]);report.measurements.push({surface:'discovery editor',...geometry});await page.screenshot({path:path.join(output,`d06-discovery-editor-${width}.png`),fullPage:true});await page.locator('[data-discovery-preview]').scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,`d06-discovery-preview-${width}.png`)});
  }
  await ui.accessibility(page,origin,'discovery editor',report);
  assert.equal((await visitors.request.get(origin+'/browser-discovery-journal')).status(),404);
  await submit(page.getByRole('button',{name:'Save draft',exact:true}));
  assert.equal((await visitors.request.get(origin+'/browser-discovery-journal')).status(),404);
  await submit(page.getByRole('button',{name:'Publish now',exact:true}));
  await visitor.goto(origin+'/browser-discovery-journal');await visitor.getByRole('heading',{name:'The local garden film',exact:true}).waitFor();
  const observed=await visitor.locator('video').evaluate(async el=>{if(el.readyState<1)await new Promise((resolve,reject)=>{el.addEventListener('loadedmetadata',resolve,{once:true});el.addEventListener('error',reject,{once:true});});return{duration:el.duration,width:el.videoWidth,height:el.videoHeight,source:el.getAttribute('src')};});assert(observed.duration>0&&observed.width>0&&observed.height>0);report.observations.push(observed);
  assert.equal(await visitor.locator('meta[property="og:image"]').getAttribute('content'),origin+'/media/'+image.id);
  for(const kind of ['image','video','news']){const r=await visitors.request.get(origin+'/discovery-sitemap/'+kind);assert(r.ok());assert((await r.text()).includes('/browser-discovery-journal'));}
  await visitor.screenshot({path:path.join(output,'d06-discovery-public.png'),fullPage:true});
  // Working metadata remains private until deliberate publication.
  await page.getByText('Language & discovery',{exact:true}).click();await page.locator('[name=seo_title]').fill('PRIVATE_WORKING_DISCOVERY');await submit(page.getByRole('button',{name:'Save draft',exact:true}));
  assert(!(await (await visitors.request.get(origin+'/browser-discovery-journal')).text()).includes('PRIVATE_WORKING_DISCOVERY'));
  csrf=await page.locator('#main [name=csrf]').first().inputValue();const revoked=await owner.request.post(origin+'/admin/media/'+image.id,{headers:{Origin:origin},form:{csrf,visibility:'private',alt:'Private garden'}});assert(revoked.ok());
  await visitor.reload();assert.equal(await visitor.locator('meta[property="og:image"]').count(),0);assert.equal(await visitor.locator('video').count(),0);
  for(const kind of ['image','video'])assert(!(await (await visitors.request.get(origin+'/discovery-sitemap/'+kind)).text()).includes('/browser-discovery-journal'));
  // Native no-JavaScript metadata edits preserve the expanded shared model.
  const native=await owner.browser().newContext({javaScriptEnabled:false,storageState:await owner.storageState(),viewport:{width:320,height:1000}});
  try{const p=await native.newPage();await p.goto(editorUrl);await p.getByText('Language & discovery',{exact:true}).click();await p.locator('summary').filter({hasText:/^Social image$/}).click();await p.locator('[name=seo_social_alt]').fill('Native private image text');await Promise.all([p.waitForNavigation({waitUntil:'load'}),p.getByRole('button',{name:'Save draft',exact:true}).click()]);await p.getByText('Language & discovery',{exact:true}).click();await p.locator('summary').filter({hasText:/^Social image$/}).click();assert.equal(await p.locator('[name=seo_social_alt]').inputValue(),'Native private image text');}finally{await native.close();}
  assert.deepEqual(report.errors,[]);report.status='passed';
 }finally{fs.writeFileSync(path.join(output,'d06-discovery.json'),JSON.stringify(report,null,2)+'\n');await page.close();await visitor.close();}
};
