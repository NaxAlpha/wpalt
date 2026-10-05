const assert=require('node:assert/strict');const fs=require('node:fs');const path=require('node:path');const ui=require('./ui_contracts.cjs');
module.exports=async(owner,origin,output)=>{
 const page=await owner.newPage();const report={measurements:[],accessibility:[],journey:[]};
 await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
 await page.route(origin+'/__ui_fixture/integration-spacing.css',r=>r.fulfill({contentType:'text/css',body:'body *{line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}p{margin-bottom:2em!important}'}));
 try{
  await page.goto(origin+'/admin/operations');await page.getByRole('link',{name:'Manage integrations',exact:true}).click();
  assert.equal(await page.locator('.sidebar nav a[aria-current="page"]').getAttribute('href'),'/admin/operations');
  const name='Independent draft worker '+ 'a'.repeat(65);
  await page.getByLabel('Integration name',{exact:true}).fill(name);
  await page.getByLabel('Allowed access',{exact:true}).selectOption('draft');
  const pending=page.waitForEvent('download');await page.getByRole('button',{name:'Create and download credential',exact:true}).click();
  const download=await pending;const token=fs.readFileSync(await download.path(),'utf8');assert.match(token,/^[0-9a-f]{64}$/);
  await page.getByRole('link',{name:'Refresh credential list',exact:true}).click();
  await page.getByRole('heading',{name,exact:true}).waitFor();
  assert.equal((await fetch(origin+'/api/v1/content',{headers:{Authorization:'Bearer '+token}})).status,200);
  for(const width of [320,768,1440]){
   await page.setViewportSize({width,height:1000});await page.evaluate(()=>scrollTo(0,0));
   const normal=await ui.geometry(page);if(normal.failures.length){const overflow=await page.evaluate(()=>[...document.querySelectorAll("body *")].filter(e=>e.getClientRects().length && (e.getBoundingClientRect().right>innerWidth+1||e.getBoundingClientRect().left< -1)).slice(0,30).map(e=>({tag:e.tagName,class:e.className,right:e.getBoundingClientRect().right,width:e.getBoundingClientRect().width,text:(e.textContent||"").slice(0,100),wrap:getComputedStyle(e).overflowWrap})));fs.writeFileSync(path.join(output,"integration-geometry-failure.json"),JSON.stringify({normal,overflow,styles:await page.evaluate(()=>({body:document.body.className,width:innerWidth,styles:[...document.styleSheets].map(s=>({href:s.href,disabled:s.disabled,rules:(()=>{try{return s.cssRules.length}catch{return "unavailable"}})()})),token:getComputedStyle(document.documentElement).getPropertyValue("--ui-type-body")}))},null,2));await page.screenshot({path:path.join(output,"integration-geometry-failure.png"),fullPage:true});}assert.deepEqual(normal.failures,[]);report.measurements.push({state:'populated',...normal});
   await page.screenshot({path:path.join(output,`integration-review-${width}.png`),fullPage:true});
   await page.screenshot({path:path.join(output,`integration-top-${width}.png`)});
   await page.getByRole('heading',{name:'Granted credentials',exact:true}).scrollIntoViewIfNeeded();
   await page.screenshot({path:path.join(output,`integration-inventory-${width}.png`)});
   await page.evaluate(()=>scrollTo(0,0));
   const style=await page.addStyleTag({url:origin+'/__ui_fixture/integration-spacing.css'});const spaced=await ui.geometry(page);assert.deepEqual(spaced.failures,[]);report.measurements.push({state:'text-spacing',...spaced});await style.evaluate(n=>n.remove());
  }
  await ui.accessibility(page,origin,'integration-review',report);
  await page.getByLabel('Integration name',{exact:true}).fill('Expiry retry');
  await page.getByLabel('Expires after',{exact:true}).evaluate(element=>{element.options[element.selectedIndex].value='0';});
  await Promise.all([page.waitForNavigation({waitUntil:'load'}),page.getByRole('button',{name:'Create and download credential',exact:true}).click()]);
  await page.getByRole('alert').filter({hasText:'expiry of 1–30 days'}).waitFor();
  assert.equal(await page.getByLabel('Integration name',{exact:true}).inputValue(),'Expiry retry');
  await Promise.all([page.waitForNavigation({waitUntil:'load'}),page.getByRole('button',{name:'Revoke '+name,exact:true}).click()]);
  await page.getByText('No integration credentials. Your site works without external tools.',{exact:true}).waitFor();
  assert.equal((await fetch(origin+'/api/v1/content',{headers:{Authorization:'Bearer '+token}})).status,403);
  await page.locator('.skip').focus();await page.keyboard.press('Tab');assert.notEqual(await page.evaluate(()=>document.activeElement.tagName),'BODY');
  report.status='passed';report.journey.push('Owner grants explicit draft access, downloads a one-time secret, reviews populated inventory across responsive/text-spacing states, corrects a readable preserved-input error, and revokes live external access. Secret excluded from screenshots/reports.');
 }finally{await page.close();fs.writeFileSync(path.join(output,'integration-result.json'),JSON.stringify(report,null,2));}
};
