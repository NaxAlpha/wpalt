// Actual native responses delayed across withdrawal/regrant, without force clicks.
const assert=require('node:assert/strict');
module.exports=async(browser,origin)=>{
  const context=await browser.newContext();const page=await context.newPage();
  let releaseOld,releaseNew,oldTargeted,newTargeted;
  const oldGate=new Promise(resolve=>{releaseOld=resolve});
  const newGate=new Promise(resolve=>{releaseNew=resolve});
  const first=new Promise(resolve=>{oldTargeted=resolve});
  const second=new Promise(resolve=>{newTargeted=resolve});
  let requests=0;
  await page.route('**/api/engagement/offers',async route=>{
    const response=await route.fetch();const result=await response.json();
    assert.equal(response.status(),200);
    const ordinal=++requests;
    if(ordinal===1){oldTargeted(result);await oldGate;}
    else if(ordinal===2){newTargeted(result);await newGate;}
    await route.fulfill({response});
  });
  const clickConsent=async name=>{
    const [response]=await Promise.all([page.waitForResponse(r=>r.url().endsWith('/api/engagement/consent')),page.getByRole('button',{name,exact:true}).click()]);
    assert.equal(response.status(),200);
  };
  try{
    await page.goto(origin+'/',{waitUntil:'networkidle'});
    await clickConsent('Allow local analytics');
    const old=await first;assert.ok(old.offer,'Native fixture must target an offer for this visitor');
    await clickConsent('Withdraw and erase my analytics');
    await page.getByRole('button',{name:'Review privacy choices',exact:true}).click();
    await clickConsent('Allow local analytics');
    const current=await second;
    const oldReply=page.waitForResponse(r=>r.url().endsWith('/api/engagement/offers'));
    releaseOld();await oldReply;
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    assert.equal(await page.locator('#local-offer').count(),0,'Reply from a withdrawn grant must not show after a newer grant');
    const newReply=page.waitForResponse(r=>r.url().endsWith('/api/engagement/offers'));
    releaseNew();await newReply;
    if(current.offer){const close=page.getByRole('button',{name:'Close offer',exact:true});await close.waitFor({state:'visible'});await close.click();}
    await clickConsent('Withdraw and erase my analytics');
    console.log('PASS: stale native offer reply cannot reappear after withdrawal/regrant; current consent remains usable.');
  }finally{releaseOld();releaseNew();await context.close();}
};
