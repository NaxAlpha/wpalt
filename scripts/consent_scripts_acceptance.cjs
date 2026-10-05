// Real browser verifies executed local code, renewed disclosure and withdrawal, not just markup.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ui = require('./ui_contracts.cjs');
module.exports = async (owner, publicContext, origin, output) => {
  const admin = await owner.newPage();
  await admin.goto(origin+'/admin/engagement');
  await admin.getByLabel('Enable consented analytics').check();
  await admin.getByLabel('Analytics purpose').fill('Understand this locally operated website.');
  await Promise.all([admin.waitForNavigation({waitUntil:'load'}),admin.getByRole('button',{name:'Save privacy settings',exact:true}).click()]);
  const page = await publicContext.newPage();
  const report={measurements:[],accessibility:[]};
  await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
  const urls=[];page.on('request',r=>{if(r.url().includes('/api/engagement/scripts/'))urls.push(r.url());});
  await page.goto(origin+'/journal-1');
  await page.getByRole('button',{name:'Allow local analytics and declared scripts',exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>window.__wpaltOptionalExample),undefined);
  assert.equal(urls.length,0,'No script request before explicit consent');
  await page.getByText('Local example: Count consented visits on this server.',{exact:true}).waitFor();
  for(const width of [320,768,1440]){await page.setViewportSize({width,height:1000});const geometry=await page.evaluate(()=>{const host=document.querySelector('#engagement-controls');return {width:innerWidth,document_width:document.documentElement.scrollWidth,bounds:host.getBoundingClientRect().toJSON(),buttons:[...host.querySelectorAll('button')].map(b=>({label:b.textContent,...b.getBoundingClientRect().toJSON()}))};});assert(geometry.document_width<=width+1,'Privacy disclosure reflows');assert(geometry.buttons.every(b=>b.width>=44 && b.height>=44),'Privacy controls retain 44px product targets');report.measurements.push(geometry);await page.screenshot({path:path.join(output,`consent-scripts-${width}.png`),fullPage:true});}
  await ui.accessibility(page,origin,'public-script-consent',report);
  const status=await (await page.request.get(origin+'/api/engagement/status')).json();
  const url=origin+`/api/engagement/scripts/${status.manifest}/example`;
  assert.equal((await page.request.get(url)).status(),403);
  await page.getByRole('button',{name:'Allow local analytics and declared scripts',exact:true}).click();
  await page.waitForFunction(()=>window.__wpaltOptionalExample===1);
  assert.equal(urls.length,1);
  const served=await page.request.get(url);assert.equal(served.status(),200);assert.equal(served.headers()['cache-control'],'no-store');
  await page.reload();await page.waitForFunction(()=>window.__wpaltOptionalExample===1);
  // Same-origin documents cannot retain running optional code after withdrawal.
  await Promise.all([page.waitForNavigation({waitUntil:'load'}),page.getByRole('button',{name:'Withdraw and erase my analytics',exact:true}).click()]);
  assert.equal(await page.evaluate(()=>window.__wpaltOptionalExample),undefined);
  assert.equal((await page.request.get(url)).status(),403);
  await page.getByRole('button',{name:'Allow local analytics and declared scripts',exact:true}).waitFor();
  await page.screenshot({path:path.join(output,'consent-scripts-withdrawn.png'),fullPage:true});
  const preference=await owner.browser().newContext({extraHTTPHeaders:{'Sec-GPC':'1'}});
  await preference.addInitScript(()=>Object.defineProperty(navigator,'globalPrivacyControl',{value:true}));
  const opted=await preference.newPage();await opted.goto(origin+'/journal-1');
  await opted.getByText('Analytics are off because of your browser privacy preference.',{exact:true}).waitFor();
  assert.equal(await opted.evaluate(()=>window.__wpaltOptionalExample),undefined);
  assert.equal((await preference.request.get(url)).status(),403);
  fs.writeFileSync(path.join(output,'consent-scripts-result.json'),JSON.stringify({...report,status:'passed',browser:owner.browser().version(),journey:['No source download or execution before explicit manifest-bound consent','Actual owner-hosted code executes once per page only after grant','Withdrawal unloads running code and prevents repeat source reads','GPC blocks source and execution']},null,2));
  await preference.close();await page.close();await admin.close();
};
