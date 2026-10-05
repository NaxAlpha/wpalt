// Optional owner-page browser worker. Reports metadata, never cookie/storage values.
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
async function scan(browser, origin, paths = ["/"], output) {
  const base = new URL(origin);
  assert.ok(["http:","https:"].includes(base.protocol));
  assert.equal(base.origin, origin, "Use an origin without path, credentials or query");
  assert.ok(paths.length>0 && paths.length<=20,"Scan 1–20 explicit owner pages");
  for (const p of paths) assert.ok(p.startsWith("/") && !p.startsWith("//") && !/[?#\\]/.test(p) && new URL(p,origin).origin===origin,"Use same-origin paths without tokens or queries");
  const report={checked_at:new Date().toISOString(),origin,pages:[],blocked_external_origins:[],limitations:"Finite owner-page sample. Blocks third-party requests; does not certify legal compliance or discover every runtime state. Cookie and storage values are excluded."};
  const foreign=new Set();
  const context=await browser.newContext();
  await context.route("**/*",async route=>{
    const url=new URL(route.request().url());
    if (url.origin!==origin && !["data:","blob:"].includes(url.protocol)) {foreign.add(url.origin);await route.abort();}else await route.continue();
  });
  const page=await context.newPage();
  async function snapshot(state){
    const cookies=(await context.cookies()).map(({name,domain,path,secure,httpOnly,sameSite,expires})=>({name,domain,path,secure,httpOnly,sameSite,expires}));
    const storage=await page.evaluate(()=>({local_storage_keys:Object.keys(localStorage).sort(),session_storage_keys:Object.keys(sessionStorage).sort(),script_sources:[...document.scripts].map(s=>s.src?new URL(s.src).pathname:"inline")}));
    return {state,cookies,...storage};
  }
  try {
    for (const p of paths) {
      await context.clearCookies();
      await page.goto(origin+p,{waitUntil:"networkidle",timeout:15000});
      await page.evaluate(()=>{localStorage.clear();sessionStorage.clear();});
      await page.reload({waitUntil:"networkidle",timeout:15000});
      const states=[await snapshot("before_consent")];
      const allow=page.getByRole("button",{name:"Allow local analytics",exact:true});
      if (await allow.count()) {
        const [response] = await Promise.all([page.waitForResponse(r=>r.url().endsWith("/api/engagement/consent")),allow.click()]);
        assert.equal(response.status(),200,"Cookie scan consent admission failed");
        await page.getByRole("button",{name:"Withdraw and erase my analytics",exact:true}).waitFor();
        await page.waitForLoadState("networkidle");
        const closeOffer=page.getByRole("button",{name:"Close offer",exact:true});
        if(await closeOffer.count())await closeOffer.click();
        states.push(await snapshot("allowed_local_analytics"));
        const [withdrawal] = await Promise.all([page.waitForResponse(r=>r.url().endsWith("/api/engagement/consent")),page.getByRole("button",{name:"Withdraw and erase my analytics",exact:true}).click()]);
        assert.equal(withdrawal.status(),200,"Cookie scan withdrawal failed");
        states.push(await snapshot("withdrawn"));
      }
      report.pages.push({path:p,states});
    }
    report.blocked_external_origins=[...foreign].sort();
    report.status="completed";
  }finally{await context.close();}
  if (output) {
    fs.mkdirSync(path.dirname(output),{recursive:true});
    fs.writeFileSync(output,JSON.stringify(report,null,2)+"\n",{mode:0o600,flag:"wx"});
  }
  return report;
}
module.exports=scan;
if (require.main===module) {
  (async()=>{
    const [origin,output,...paths]=process.argv.slice(2);
    if(!origin||!output)throw Error("Usage: node scripts/cookie_scan.cjs ORIGIN NEW_PRIVATE_REPORT [PATH ...]");
    const {chromium}=require(process.env.PLAYWRIGHT_MODULE||"playwright");
    const browser=await chromium.launch({headless:true,...(process.env.CHROME_EXECUTABLE?{executablePath:process.env.CHROME_EXECUTABLE}:{})});
    try{await scan(browser,origin,paths.length?paths:["/"],output);}finally{await browser.close();}
  })().catch(e=>{console.error(e.message);process.exitCode=1;});
}
