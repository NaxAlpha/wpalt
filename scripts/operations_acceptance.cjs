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
  try {
    await page.goto(origin + "/account/security");
    const register = page.locator('[data-passkey="register"]');
    await register.getByLabel("Current password", {exact:true}).fill(password);
    const finish = page.waitForResponse(r => r.url().endsWith("/account/passkeys/finish"), {timeout:15000});
    await register.getByRole("button",{name:"Register a passkey"}).click();
    assert.equal((await finish).status(), 200);
    await page.waitForURL(origin + "/account/security");
    await page.getByRole("button",{name:"Remove passkey & revoke sessions"}).waitFor();
    report.journey.push("User-verified local passkey registration");
    for (const width of [320,768,1440]) {
      await page.setViewportSize({width,height:1000});
      const m = await ui.geometry(page);
      assert.deepEqual(m.failures, [], `Account security at ${width}: ${m.failures.join("; ")}`);
      report.measurements.push({surface:"account security",width,...m});
      await page.screenshot({path:path.join(output,`security-${width}.png`),fullPage:true});
    }
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
    report.status="passed";
  } finally {
    await cdp.send("WebAuthn.removeVirtualAuthenticator",{authenticatorId});
    await page.close();
    fs.writeFileSync(path.join(output,"operations-result.json"),JSON.stringify(report,null,2));
  }
};
