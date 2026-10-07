// A connected writing/review/publishing journey, plus representative responsive measurements.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const crypto=require('node:crypto');
const ui=require('./ui_contracts.cjs');
module.exports=async(owner,publicContext,origin,output)=>{
 const page=await owner.newPage();const reviewerContext=await owner.browser().newContext({viewport:{width:1440,height:1000}});const review=await reviewerContext.newPage();
 const errors=[],remote=[],measurements=[],accessibility=[];
 for(const context of [reviewerContext]){context.on('page',p=>p.on('pageerror',e=>errors.push(e.message)));context.on('request',r=>{if(!r.url().startsWith(origin))remote.push(r.url());});}
 review.on('pageerror',e=>errors.push(e.message));
 const submit=async(page,button)=>{const [response]=await Promise.all([page.waitForResponse(r=>r.request().method()==='POST'&&new URL(r.url()).pathname.startsWith('/admin/')),page.waitForNavigation({waitUntil:'load'}),button.click()]);if(response.status()>=400)throw new Error(`Editorial action failed (${response.status()}): ${await response.text()}`);};
 try{
  await page.goto(origin+'/admin');const csrf=await page.locator('input[name=csrf]').first().inputValue();
  const password=crypto.randomBytes(24).toString('hex'),email='review-'+crypto.randomBytes(6).toString('hex')+'@example.test';
  let response=await owner.request.post(origin+'/admin/users',{headers:{Origin:origin},form:{csrf,name:'Editorial reviewer',email,role:'editor',password}});assert(response.status()<400,await response.text());
  await page.goto(origin+'/admin/builder');await page.getByRole('button',{name:'models',exact:true}).click();
  await page.getByLabel('Content model',{exact:true}).selectOption('post');
  await page.getByLabel('Require assigned review before publication',{exact:true}).check();
  await page.getByRole('button',{name:'Save model',exact:true}).click();
  await page.getByText('Model saved after existing-data validation',{exact:true}).waitFor();
  await page.goto(origin+'/admin/posts/new');
  await page.getByLabel('Title',{exact:true}).fill('A reviewed editorial story');
  await page.getByLabel('URL slug',{exact:true}).fill('reviewed-editorial-story');
  await page.locator('.ProseMirror').fill('EDITORIAL_EXACT_WORKING_COPY');
  await page.locator('[data-editorial-pane] summary').click();
  const reviewer=await page.getByLabel('Reviewer',{exact:true}).locator('option').evaluateAll((options,email)=>options.find(o=>o.textContent.includes(email)).value,email);
  await page.getByLabel('Reviewer',{exact:true}).selectOption(reviewer);
  await page.getByLabel('Private review note',{exact:true}).fill('PRIVATE_EDITORIAL_REQUEST');
  await submit(page,page.getByRole('button',{name:'Save & request review',exact:true}));
  const editorUrl=page.url();assert.match(editorUrl,/\/admin\/posts\/[a-f0-9-]+$/);
  assert.equal((await publicContext.request.get(origin+'/reviewed-editorial-story')).status(),404);
  await review.goto(origin+'/login');await review.getByLabel('Email',{exact:true}).fill(email);await review.getByLabel('Password',{exact:true}).fill(password);
  await Promise.all([review.waitForNavigation({waitUntil:'load'}),review.getByRole('button',{name:'Sign in',exact:true}).click()]);
  await review.route(origin+'/__ui_fixture/axe.js',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  await review.goto(origin+'/admin/editorial');await review.getByRole('link',{name:'A reviewed editorial story',exact:true}).waitFor();
  await review.getByText('PRIVATE_EDITORIAL_REQUEST',{exact:true}).waitFor();
  for(const width of [320,768,1440]){await review.setViewportSize({width,height:1000});const m=await ui.geometry(review);assert.deepEqual(m.failures,[],`Editorial queue ${width}`);measurements.push({surface:'assigned queue',...m});await review.screenshot({path:path.join(output,`d02-queue-${width}.png`),fullPage:true});}
  await ui.accessibility(review,origin,'assigned editorial queue',{accessibility});
  await review.getByRole('link',{name:'A reviewed editorial story',exact:true}).click();
  await review.getByLabel('Decision note',{exact:true}).fill('PRIVATE_EDITORIAL_APPROVAL');
  for(const width of [320,1440]){await review.setViewportSize({width,height:1000});const m=await ui.geometry(review);assert.deepEqual(m.failures,[],`Reviewer editor ${width}`);measurements.push({surface:'review decision',...m});await review.screenshot({path:path.join(output,`d02-decision-${width}.png`),fullPage:true});}
  await ui.accessibility(review,origin,'assigned reviewer editor',{accessibility});
  await submit(review,review.getByRole('button',{name:'Approve saved content',exact:true}));
  await page.goto(editorUrl);await page.getByText('PRIVATE_EDITORIAL_APPROVAL',{exact:true}).first().waitFor();
  await submit(page,page.getByRole('button',{name:'Publish now',exact:true}));
  response=await publicContext.request.get(origin+'/reviewed-editorial-story');assert.equal(response.status(),200);const html=await response.text();assert(html.includes('EDITORIAL_EXACT_WORKING_COPY'));assert(!html.includes('PRIVATE_EDITORIAL'));
  // Saving a changed working copy leaves the existing live document intact and removes approval.
  await page.locator('.ProseMirror').fill('EDITORIAL_UNREVIEWED_NEW_WORK');
  await submit(page,page.getByRole('button',{name:'Save draft',exact:true}));
  const postId=editorUrl.split('/').pop();response=await owner.request.get(origin+'/api/admin/editorial/'+postId);assert.equal(response.status(),200);assert.equal((await response.json()).state,'draft');
  response=await publicContext.request.get(origin+'/reviewed-editorial-story');const stillLive=await response.text();assert(stillLive.includes('EDITORIAL_EXACT_WORKING_COPY'));assert(!stillLive.includes('EDITORIAL_UNREVIEWED_NEW_WORK'));
  assert.deepEqual(errors,[]);assert.deepEqual(remote,[]);
  fs.writeFileSync(path.join(output,'d02-editorial-measurements.json'),JSON.stringify({browser:owner.browser().version(),measurements,accessibility,errors,remote},null,2));
  console.log('PASS: assigned editorial review, exact live projection, private feedback, approval invalidation and responsive queue/decision controls');
 }finally{
  // Restore this fixture's publication policy so other cumulative journeys retain their deliberate baseline.
  const state=await(await owner.request.get(origin+'/api/admin/design')).json();
  if(state.registry?.models?.post?.review_required){await page.goto(origin+'/admin');const csrf=await page.locator('input[name=csrf]').first().inputValue();const response=await owner.request.post(origin+'/api/admin/models/post',{headers:{Origin:origin},data:{csrf,version:state.model_versions.post,definition:{...state.registry.models.post,review_required:false}}});assert.equal(response.status(),200,await response.text());}
  await reviewerContext.close();await page.close();
 }
};
