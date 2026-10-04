// Real browser passkey ceremony; Chromium's virtual device signs the challenge.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, origin, output, password) => {
  const page = await owner.newPage();
  const errors = [];
  page.on("pageerror", e => errors.push(e.message));
  const cdp = await owner.newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  const {authenticatorId} = await cdp.send("WebAuthn.addVirtualAuthenticator", {options:{protocol:"ctap2",transport:"internal",hasResidentKey:true,hasUserVerification:true,isUserVerified:true,automaticPresenceSimulation:true}});
  const report = {journey:[],measurements:[],accessibility:[]};
  await page.route(origin+"/__ui_fixture/axe.js",r=>r.fulfill({contentType:"text/javascript",body:fs.readFileSync(path.join(__dirname,"../frontend/node_modules/axe-core/axe.min.js"))}));
  await page.route(origin+"/__ui_fixture/operations-spacing.css",r=>r.fulfill({contentType:"text/css",body:"body * {line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}p{margin-bottom:2em!important}"}));
  const measure=async name=>{
    for(const width of [320,768,1440]) {
      await page.setViewportSize({width,height:1000});
      const m=await ui.geometry(page);assert.deepEqual(m.failures,[],`${name} at ${width}: ${m.failures.join("; ")}`);
      report.measurements.push({surface:name,state:"normal",...m});
      await page.screenshot({path:path.join(output,`operations-${name}-${width}.png`),fullPage:true});
      const style=await page.addStyleTag({url:origin+"/__ui_fixture/operations-spacing.css"});
      const spaced=await ui.geometry(page);assert.deepEqual(spaced.failures,[],`${name} text spacing at ${width}`);
      report.measurements.push({surface:name,state:"text-spacing",...spaced});await style.evaluate(n=>n.remove());
    }
    await ui.accessibility(page,origin,name,report);
    await page.keyboard.press("Tab");
    assert.notEqual(await page.evaluate(()=>document.activeElement.tagName),"BODY","Keyboard reaches a control");
  };

  try {
    await page.goto(origin+"/admin/operations");
    await measure("operations");
    await Promise.all([page.waitForNavigation(),page.getByRole("button",{name:"Inspect stored-file integrity",exact:true}).click()]);
    await page.getByRole("heading",{name:"Stored-file inspection",exact:true}).waitFor();
    await measure("integrity");
    await page.goto(origin+"/admin/operations/audit");
    await measure("audit");
    await page.goto(origin + "/account/security");
    const register = page.locator('[data-passkey="register"]');
    await register.getByLabel("Current password", {exact:true}).fill(password);
    const finish = page.waitForResponse(r => r.url().endsWith("/account/passkeys/finish"), {timeout:15000});
    await register.getByRole("button",{name:"Register a passkey"}).click();
    assert.equal((await finish).status(), 200);
    await page.waitForURL(origin + "/account/security");
    await page.getByRole("button",{name:"Remove passkey & revoke sessions"}).waitFor();
    report.journey.push("User-verified local passkey registration");
    await measure("account-security");
    await owner.clearCookies();
    await page.goto(origin + "/login");
    await page.getByLabel("Passkey account",{exact:true}).fill("owner@example.test");
    const signedIn = page.waitForResponse(r=>r.url().endsWith("/passkeys/login/finish"));
    await page.getByRole("button",{name:"Use a passkey"}).click();
    const accepted = await signedIn;
    assert.equal(accepted.status(),200);
    await page.waitForURL(origin + "/admin");
    const replay = await page.evaluate(async body => {
      const r=await fetch("/passkeys/login/finish",{method:"POST",headers:{"content-type":"application/json"},body}); return r.status;
    },accepted.request().postData());
    assert.equal(replay,403,"Signed challenge can be consumed only once");
    await page.waitForURL(origin + "/admin");
    report.journey.push("Real browser signature signs in without password");
    await page.goto(origin + "/account/security");
    const remove = page.locator('form[action="/account/passkeys/remove"]');
    await remove.getByLabel("Current password",{exact:true}).fill(password);
    await Promise.all([page.waitForNavigation(),remove.getByRole("button").click()]);
    assert.equal(await page.locator('form[action="/account/passkeys/remove"]').count(),0);
    const denied = await page.evaluate(async origin => {
      const r = await fetch(origin + "/passkeys/login/start",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({email:"owner@example.test"})});return r.status;
    },origin);
    assert.equal(denied,403);
    report.journey.push("Removed key cannot begin a new sign-in");
    assert.deepEqual(errors,[]);
    const cookieReport = await require("./cookie_scan.cjs")(owner.browser(),origin,["/","/about"]);
    assert.equal(cookieReport.pages.length,2);
    assert.deepEqual(cookieReport.blocked_external_origins,[]);
    assert.ok(!JSON.stringify(cookieReport).includes(password),"Cookie report contains no credential values");
    report.cookie_scan=cookieReport;
    report.status="passed";
  } finally {
    await cdp.send("WebAuthn.removeVirtualAuthenticator",{authenticatorId});
    await page.close();
    fs.writeFileSync(path.join(output,"operations-result.json"),JSON.stringify(report,null,2));
  }
};
