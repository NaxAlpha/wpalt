// Reviewable real-browser workflow. The app needs no Node runtime; this tester does.
const fs = require('node:fs');
const path = require('node:path');
const net = require('node:net');
const crypto = require('node:crypto');
const {spawn, spawnSync} = require('node:child_process');
const assert = require('node:assert/strict');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root = path.resolve(__dirname, '..');
const output = path.join(root, 'work/browser-evidence');
fs.mkdirSync(output, {recursive: true});
const temporary = fs.mkdtempSync(path.join(root, 'work/browser-site-'));
const binary = path.resolve(process.env.WPALT_BINARY || path.join(root, 'target/debug/wpalt'));
const password = crypto.randomBytes(24).toString('hex');
let server, browser, logFd;
function command(config, args, input) {
  const r = spawnSync(binary, ['--config', config, ...args], {input, encoding: 'utf8'});
  assert.equal(r.status, 0, r.stderr);
}
async function submit(page,button) {await Promise.all([page.waitForNavigation({waitUntil:'load'}),button.click()]);}
async function freePort() {
  const s = net.createServer(); await new Promise(resolve => s.listen(0, '127.0.0.1', resolve));
  const p = s.address().port; await new Promise(resolve => s.close(resolve)); return p;
}
(async () => {
  const port = await freePort(), origin = `http://127.0.0.1:${port}`;
  const config = path.join(temporary, 'site.toml');
  fs.writeFileSync(config, `database_url = "sqlite://${temporary}/site.db?mode=rwc"\ndata_dir = "${temporary}/data"\nlisten = "127.0.0.1:${port}"\nbase_url = "${origin}"\n`);
  command(config, ['init', '--admin-email', 'owner@example.test'], password+'\n');
  command(config, ['seed-demo']);
  logFd = fs.openSync(path.join(temporary, 'server.log'), 'w');
  server = spawn(binary, ['--config', config, 'serve'], {stdio: ['ignore', logFd, logFd]});
  for (let i=0; i<100; i++) {
    try { if ((await fetch(origin+'/health')).ok) break; } catch (_) {}
    assert.equal(server.exitCode, null, 'Server failed to start');
    if (i===99) throw Error('Server readiness timed out');
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  browser = await chromium.launch({headless: true, ...(process.env.CHROME_EXECUTABLE ? {executablePath:process.env.CHROME_EXECUTABLE} : {})});
  const owner = await browser.newContext({viewport:{width:1440,height:1000}});
  const publicContext = await browser.newContext({viewport:{width:1440,height:1000}});
  const page = await owner.newPage(), visitor = await publicContext.newPage();
  const errors=[], remote=[];
  for (const context of [owner,publicContext]) {
    context.on('page', p=>p.on('pageerror',e=>errors.push(e.message)));
    context.on('request',r=>{if (!r.url().startsWith(origin)) remote.push(r.url());});
  }
  page.on('pageerror',e=>errors.push(e.message)); visitor.on('pageerror',e=>errors.push(e.message));
  await visitor.goto(origin); await visitor.getByRole('heading',{name:'The Local Journal',exact:true}).waitFor();
  await visitor.screenshot({path:path.join(output,'public-desktop.png'),fullPage:true});
  await page.goto(origin+'/login');
  await page.getByLabel('Email',{exact:true}).fill('owner@example.test');
  await page.getByLabel('Password',{exact:true}).fill(password);
  await submit(page,page.getByRole('button',{name:'Sign in',exact:true})); await page.waitForURL(origin+'/admin');
  await page.screenshot({path:path.join(output,'admin-desktop.png'),fullPage:true});
  await page.getByRole('link',{name:'Create content',exact:true}).click();
  await page.getByLabel('Title',{exact:true}).fill('A browser-tested story');
  await page.getByRole('textbox',{name:/^URL slug/}).fill('browser-story');
  await page.getByRole('textbox',{name:/^Content/}).fill('BROWSER_WORKING_DRAFT\n\nA useful story from our independent website.');
  await page.getByLabel('Categories',{exact:true}).fill('Browser journeys');
  await submit(page,page.getByRole('button',{name:'Save draft',exact:true}));
  await page.waitForURL(/\/admin\/posts\/[a-f0-9-]+$/);
  const editorUrl=page.url();
  assert.equal((await publicContext.request.get(origin+'/browser-story')).status(),404);
  await submit(page,page.getByRole('button',{name:'Publish now',exact:true})); await page.waitForURL(editorUrl);
  await visitor.goto(origin+'/browser-story'); await visitor.getByText('BROWSER_WORKING_DRAFT',{exact:false}).waitFor();
  await page.getByRole('textbox',{name:/^Content/}).fill('BROWSER_PRIVATE_AUTOSAVE');
  await page.getByText('Draft saved · live page unchanged',{exact:true}).waitFor({timeout:15000});
  await visitor.reload(); assert(!(await visitor.content()).includes('BROWSER_PRIVATE_AUTOSAVE'));
  const revisions=page.locator('form[action*="/revisions/"]');
  await submit(page,revisions.last().getByRole('button',{name:'Restore working copy'})); await page.waitForURL(editorUrl);
  assert((await page.getByRole('textbox',{name:/^Content/}).inputValue()).includes('BROWSER_WORKING_DRAFT'));
  await page.screenshot({path:path.join(output,'editor-desktop.png'),fullPage:true});
  await page.goto(origin+'/admin/media');
  await page.getByLabel('Image',{exact:true}).setInputFiles(path.join(root,'tests/fixtures/green.png'));
  await page.getByLabel('Visibility',{exact:true}).selectOption('private');
  await page.getByLabel('Alternative text',{exact:true}).fill('A green example image');
  await submit(page,page.getByRole('button',{name:'Upload image',exact:true}));
  const card=page.locator('.media-card').first();await card.waitFor();
  const mediaUrl=await card.locator('img').getAttribute('src');assert.equal((await publicContext.request.get(origin+mediaUrl)).status(),401);
  await card.getByLabel('Visibility',{exact:true}).selectOption('public');
  await submit(page,card.getByRole('button',{name:'Save details',exact:true}));
  await page.waitForURL(origin+'/admin/media');assert.equal((await publicContext.request.get(origin+mediaUrl)).status(),200);
  await visitor.goto(origin+'/browser-story');
  await visitor.getByLabel('Your name',{exact:true}).fill('A reader');
  await visitor.getByLabel('Comment',{exact:true}).fill('BROWSER_COMMENT_TO_REVIEW');
  await submit(visitor,visitor.getByRole('button',{name:'Submit for review',exact:true}));
  await visitor.getByRole('heading',{name:'Thank you for joining in.',exact:true}).waitFor();
  await page.goto(origin+'/admin/comments');await submit(page,page.getByRole('button',{name:'Approve',exact:true}));
  await visitor.goto(origin+'/browser-story');await visitor.getByText('BROWSER_COMMENT_TO_REVIEW',{exact:true}).waitFor();
  await page.goto(origin+'/admin/settings');await page.getByLabel('Theme',{exact:true}).selectOption('ink');
  await submit(page,page.getByRole('button',{name:'Save site settings',exact:true}));await page.waitForURL(origin+'/admin/settings');
  await visitor.goto(origin);assert.equal(await visitor.locator('body').getAttribute('class'),'ink');
  await visitor.screenshot({path:path.join(output,'public-ink.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});await page.goto(editorUrl);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,'Admin has mobile body overflow');
  await page.screenshot({path:path.join(output,'editor-mobile.png'),fullPage:true});
  await visitor.setViewportSize({width:390,height:844});await visitor.goto(origin+'/browser-story');
  assert.equal(await visitor.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,'Public page has mobile body overflow');
  await visitor.screenshot({path:path.join(output,'public-mobile.png'),fullPage:true});
  assert.deepEqual(errors,[],'Browser JavaScript errors');assert.deepEqual(remote,[],'Unexpected external runtime requests');
  fs.writeFileSync(path.join(output,'result.json'),JSON.stringify({status:'passed',journeys:['public rendering','login/admin','draft/publish','isolated autosave','revision restore','private/public image','comment moderation','theme switch','responsive layout'],external_requests:remote.length,script_errors:errors.length},null,2));
  console.log('PASS: real browser authoring, autosave/revision isolation, media permissions, moderation, theme switching and mobile layouts; no external requests or script errors.');
})().catch(async e=>{
  console.error(e);
  if(browser){const p=browser.contexts()[0]?.pages()[0];if(p){await p.screenshot({path:path.join(output,'failure.png'),fullPage:true});console.error('UI status:',await p.locator('[data-save-status]').textContent().catch(()=>''),'UI error:',await p.locator('[data-editor-error]').textContent().catch(()=>''));}}
  process.exitCode=1;
}).finally(async()=>{
  if(browser)await browser.close();
  if(server && server.exitCode===null){server.kill('SIGTERM');await new Promise(resolve=>server.once('exit',resolve));}
  if(logFd!==undefined)fs.closeSync(logFd);
  fs.rmSync(temporary,{recursive:true,force:true});
});
