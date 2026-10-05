const assert=require('node:assert/strict');const fs=require('node:fs');const path=require('node:path');const ui=require('./ui_contracts.cjs');
module.exports=async(owner,origin,output)=>{
 const page=await owner.newPage();const report={measurements:[],accessibility:[],journey:[]};
 await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
 await page.route(origin+'/__ui_fixture/migration-spacing.css',r=>r.fulfill({contentType:'text/css',body:'body *{line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}p{margin-bottom:2em!important}'}));
 try{
  await page.goto(origin+'/admin/migration');
  await page.getByLabel('WordPress export',{exact:true}).setInputFiles(path.join(__dirname,'../tests/fixtures/wordpress-core.xml'));
  await Promise.all([page.waitForNavigation({waitUntil:'load'}),page.getByRole('button',{name:'Assess export',exact:true}).click()]);
  await page.getByRole('heading',{name:'Assessment',exact:true}).waitFor();
  await page.getByText('4 source records · 2 supported core content records',{exact:true}).waitFor();
  await page.getByText('Unsupported order state',{exact:true}).waitFor();
  for(const width of [320,768,1440]){
   await page.setViewportSize({width,height:1000});await page.evaluate(()=>scrollTo(0,0));
   const normal=await ui.geometry(page);assert.deepEqual(normal.failures,[]);report.measurements.push({state:'populated',...normal});
   await page.screenshot({path:path.join(output,`migration-review-${width}.png`),fullPage:true});
   const style=await page.addStyleTag({url:origin+'/__ui_fixture/migration-spacing.css'});const spaced=await ui.geometry(page);assert.deepEqual(spaced.failures,[]);report.measurements.push({state:'text-spacing',...spaced});await style.evaluate(n=>n.remove());
  }
  await ui.accessibility(page,origin,'migration-review',report);
  await page.getByLabel('WordPress export',{exact:true}).setInputFiles(path.join(__dirname,'../tests/fixtures/wordpress-core.xml'));
  const downloadPromise=page.waitForEvent('download');await page.getByRole('button',{name:'Download full assessment',exact:true}).click();const download=await downloadPromise;
  const assessment=JSON.parse(fs.readFileSync(await download.path(),'utf8'));assert.equal(assessment.source_items,4);assert.equal(assessment.supported_core_items,2);
  await page.getByLabel('WordPress export',{exact:true}).setInputFiles({name:'invalid.xml',mimeType:'application/xml',buffer:Buffer.from('<!DOCTYPE rss><rss/>')});
  await Promise.all([page.waitForNavigation({waitUntil:'load'}),page.getByRole('button',{name:'Assess export',exact:true}).click()]);
  await page.getByRole('alert').filter({hasText:'Invalid or unsupported WordPress'}).waitFor();
  await page.getByLabel('WordPress export',{exact:true}).setInputFiles(path.join(__dirname,'../tests/fixtures/wordpress-core.xml'));
  await Promise.all([page.waitForNavigation({waitUntil:'load'}),page.getByRole('button',{name:'Assess export',exact:true}).click()]);await page.getByRole('heading',{name:'Assessment',exact:true}).waitFor();
  await page.locator('.skip').focus();await page.keyboard.press('Tab');assert.notEqual(await page.evaluate(()=>document.activeElement.tagName),'BODY');
  report.status='passed';report.journey.push('Owner assesses real multipart WXR, reviews core/unsupported records responsively, downloads complete private assessment, receives meaningful malformed-source error and successfully retries.');
 }finally{await page.close();fs.writeFileSync(path.join(output,'migration-result.json'),JSON.stringify(report,null,2));}
};
