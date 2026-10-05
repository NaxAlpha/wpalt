const assert=require('node:assert/strict');
const fs=require('node:fs');const path=require('node:path');const ui=require('./ui_contracts.cjs');
module.exports=async(owner,origin,output)=>{
  const page=await owner.newPage();const report={journey:[],measurements:[],accessibility:[]};
  await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  await page.route(origin+'/__ui_fixture/clone-spacing.css',r=>r.fulfill({contentType:'text/css',body:'body *{line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}p{margin-bottom:2em!important}'}));
  try{
    await page.goto(origin+'/admin/operations');await page.getByRole('heading',{name:'Read-only recovered clone',exact:true}).waitFor();
    for(const width of [320,768,1440]){
      await page.setViewportSize({width,height:1000});await page.evaluate(()=>{scrollTo(0,0);return new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));});
      const normal=await ui.geometry(page);assert.deepEqual(normal.failures,[]);report.measurements.push({state:'normal',...normal});
      await page.screenshot({path:path.join(output,`clone-held-${width}.png`),fullPage:true});
      const style=await page.addStyleTag({url:origin+'/__ui_fixture/clone-spacing.css'});const spaced=await ui.geometry(page);assert.deepEqual(spaced.failures,[]);report.measurements.push({state:'text-spacing',...spaced});await style.evaluate(n=>n.remove());
    }
    await ui.accessibility(page,origin,'held-clone',report);await page.locator('.skip').focus();await page.keyboard.press('Tab');assert.notEqual(await page.evaluate(()=>document.activeElement.tagName),'BODY');
    const blocked=await page.evaluate(async()=>{const r=await fetch('/admin/recovery/run',{method:'POST'});return{status:r.status,text:await r.text()};});
    assert.equal(blocked.status,503);assert(blocked.text.includes('read-only'));
    assert.equal((await owner.request.get(origin+'/members/identity/callback?code=synthetic')).status(),503);
    report.journey.push('Owner signs into fresh held clone, reviews responsive hold and receives explicit mutation denial; identity callback stays blocked.');report.status='passed';
  }finally{await page.close();fs.writeFileSync(path.join(output,'clone-result.json'),JSON.stringify(report,null,2));}
};
