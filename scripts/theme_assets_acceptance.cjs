// A connected owner/visitor font + navigation journey, with real measurements.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
module.exports = async (owner, visitors, origin, output) => {
  const admin = await owner.newPage(), page = await visitors.newPage();
  const report = { status: 'incomplete', browser:visitors.browser().version(), measurements: [],library:[],errors:[] };
  admin.on('pageerror',error=>report.errors.push(error.message));
  let previous, csrf;
  try {
    await admin.goto(origin + '/admin/design-assets');
    csrf = await admin.locator('input[name=csrf]').first().inputValue();
    const state = await (await owner.request.get(origin + '/api/admin/design')).json();
    previous = state.active;
    const font = fs.readFileSync(path.join(__dirname, '../tests/fixtures/fonts/Aboreto-Regular.ttf'));
    await admin.getByLabel('Font label', { exact: true }).fill('Aboreto browser fixture');
    await admin.getByLabel('Static TrueType font', { exact: true }).setInputFiles(path.join(__dirname, '../tests/fixtures/fonts/Aboreto-Regular.ttf'));
    await admin.getByLabel('Source / provenance', { exact: true }).fill('Official Google Fonts unmodified fixture');
    await admin.getByLabel('License / distribution permission', { exact: true }).fill(fs.readFileSync(path.join(__dirname, '../tests/fixtures/fonts/OFL.txt'), 'utf8'));
    await admin.locator('input[name=rights]').check();
    await Promise.all([admin.waitForNavigation(), admin.getByRole('button', { name: 'Upload local font', exact: true }).click()]);
    await admin.getByRole('heading', { name: 'Aboreto browser fixture', exact: true }).waitFor();
    for (const width of [320, 768, 1440]) {
      await admin.setViewportSize({ width, height: 1000 });
      await admin.getByText('Provenance and license', { exact: true }).click();
      assert(await admin.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'Expanded font license must not overflow the owner viewport');
      await admin.getByText('Provenance and license', { exact: true }).focus();
      await admin.keyboard.press('Tab');
      const license=admin.locator('pre.font-license');
      const geometry=await license.evaluate(el=>({viewport:innerWidth,height:el.getBoundingClientRect().height,focused:document.activeElement===el,outline:parseFloat(getComputedStyle(el).outlineWidth),scrollable:el.scrollHeight>el.clientHeight}));
      assert(geometry.focused && geometry.outline>=3 && geometry.scrollable && geometry.height<=400,'Long license keeps a bounded keyboard-scrollable surface with visible focus');
      report.library.push(geometry);
      await admin.screenshot({ path: path.join(output, `d04-font-library-${width}.png`), fullPage: true });
      await admin.getByText('Provenance and license', { exact: true }).click();
    }
    const inventory = await (await owner.request.get(origin + '/api/admin/design')).json();
    const admitted = inventory.fonts.find(value => value.label === 'Aboreto browser fixture');
    assert(admitted, 'Uploaded actual font appears in studio inventory');
    const url = origin + '/theme-assets/' + admitted.id;
    assert.equal((await visitors.request.get(url)).status(), 404);
    const preview = await owner.request.get(url);
    assert.deepEqual(await preview.body(), font);
    await admin.goto(origin+'/admin/discovery');
    const languageVersion=await admin.locator('input[name=version]').first().inputValue();
    const languageResponse=await owner.request.post(origin+'/admin/discovery/languages',{headers:{Origin:origin},form:{csrf,version:languageVersion,code:'ar',label:'العربية',direction:'rtl',search_label:'بحث',navigation:'[]'}});
    assert.equal(languageResponse.status(),200,await languageResponse.text());
    const package = JSON.parse(fs.readFileSync(path.join(__dirname, '../examples/themes/field-journal.json'), 'utf8'));
    package.fonts = { local: { asset: admitted.id, weight: admitted.inspection.weight, style: 'normal', display: 'swap', fallback: 'serif' } };
    package.tokens.font = 'local:local';
    package.styles={reading:{font_size:18,line_height:170,align:'start',padding:16,radius:8}};
    package.templates.home.style={};package.templates.home.style_ref='reading';
    package.navigations = { primary: { language: 'en', direction: 'ltr', label: 'Fixture navigation', layout:'columns', languages: { ar:{direction:'rtl',label:'التنقل',items:[{label:'استكشف',url:'',description:'',children:[{label:'أحدث المقالات',url:'/ar/',description:'',children:[]}]}]} }, items: [{ label: 'Explore', url: '', description: 'Local grouped navigation', children: [{ label: 'Latest stories', url: '/', description: '', children: [] }] }] } };
    package.navigations.primary.items.push({label:'Resources',url:'',description:'Long labels remain usable',children:[{label:'Documentation for independent theme authors and local site owners',url:'/',description:'',children:[]}]},{label:'About this publication',url:'/',description:'',children:[]});
    const assign = node => { if (node.kind === 'navigation') node.source = 'primary'; for (const child of node.children || []) assign(child); };
    assign(package.header);
    const save = (version, publish) => owner.request.post(origin + '/api/admin/design/d04-browser', { headers: { Origin: origin }, data: { csrf, version, publish, package } });
    let response = await save(0, false); assert.equal(response.status(), 200, await response.text());
    assert.equal((await visitors.request.get(url)).status(), 404);
    response = await save(1, true); assert.equal(response.status(), 200, await response.text());
    response = await owner.request.post(origin + '/api/admin/design/d04-browser/activate', { headers: { Origin: origin }, data: { csrf } });
    assert.equal(response.status(), 200, await response.text());
    const publicFont = await visitors.request.get(url);
    assert.deepEqual(await publicFont.body(), font);
    const conditional = await visitors.request.get(url, { headers: { 'If-None-Match': publicFont.headers().etag } });
    assert.equal(conditional.status(), 304); assert.equal((await conditional.body()).length, 0);
    for (const width of [320, 768, 1440]) {
      await page.setViewportSize({ width, height: 1000 }); await page.goto(origin + '/');
      await page.evaluate(() => document.fonts.ready);
      assert(await page.evaluate(() => document.fonts.check('16px wpaltfont_local')), 'Real browser font must finish loading');
      const summary = page.locator('.theme-navigation summary').first();
      await summary.focus(); await page.keyboard.press('Enter');
      assert(await summary.evaluate(el => el.parentElement.open));
      await page.getByRole('link', { name: 'Latest stories', exact: true }).focus();
      await page.keyboard.press('Escape');
      assert(await summary.evaluate(el => !el.parentElement.open && document.activeElement === el));
      await summary.click();
      for(const group of await page.locator('.theme-navigation summary').all())if(!(await group.evaluate(el=>el.parentElement.open)))await group.click();
      const measured = await page.evaluate(() => ({ viewport: innerWidth, overflow: document.documentElement.scrollWidth > innerWidth + 1, controls: [...document.querySelectorAll('.theme-navigation a,.theme-navigation summary')].filter(el => el.checkVisibility()).map(el => { const rect = el.getBoundingClientRect(); return { label: el.textContent, x: rect.x, width: rect.width, height: rect.height }; }), font: getComputedStyle(document.body).fontFamily, rootFont:getComputedStyle(document.querySelector('.n-home')).fontSize }));
      assert(!measured.overflow); for (const control of measured.controls) assert(control.height >= 43.5 && control.x >= 0 && control.x + control.width <= width + 1);
      report.measurements.push(measured);
      await page.screenshot({ path: path.join(output, `d04-font-navigation-${width}.png`), fullPage: true });
    }
    await page.setViewportSize({width:320,height:1000});await page.goto(origin+'/ar/');
    assert.equal(await page.locator('.theme-navigation').getAttribute('dir'),'rtl');
    await page.locator('.theme-navigation summary').click();
    assert(await page.getByRole('link',{name:'أحدث المقالات',exact:true}).isVisible());
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
    await page.screenshot({path:path.join(output,'d04-navigation-rtl-320.png'),fullPage:true});
    await admin.goto(origin+'/admin/builder');
    await admin.getByLabel('Reusable style',{exact:true}).waitFor();
    assert.equal(await admin.getByLabel('Reusable style',{exact:true}).inputValue(),'reading');
    await admin.getByLabel('font size',{exact:true}).fill('20');
    await admin.getByRole('button',{name:'Save draft',exact:true}).click();
    await admin.getByRole('status').filter({hasText:'Draft saved.'}).waitFor();
    const draftRoot=admin.frameLocator('iframe[title="Website draft preview"]').locator('.n-home');
    await draftRoot.waitFor();
    await draftRoot.evaluate(el=>document.fonts.ready);
    assert.equal(await draftRoot.evaluate(el=>getComputedStyle(el).fontSize),'20px');
    const exported=await owner.request.get(origin+'/api/admin/design/d04-browser/bundle?draft=true');
    assert.equal(exported.status(),200);const exportedBundle=await exported.json();
    assert.equal(exportedBundle.package.styles.reading.font_size,20);
    const imported=await owner.request.post(origin+'/api/admin/design/browser-portable/bundle',{headers:{Origin:origin},data:{csrf,version:0,publish:false,rights:true,bundle:exportedBundle}});
    assert.equal(imported.status(),200,await imported.text());
    assert.notEqual((await visitors.request.get(origin+'/admin/design/browser-portable/preview',{maxRedirects:0})).status(),200);
    report.shared_style_edit={draft_font_px:20,public_font_px:18};
    const noScript = await visitors.browser().newContext({ javaScriptEnabled: false });
    try { const native = await noScript.newPage(); await native.goto(origin + '/'); await native.locator('.theme-navigation summary').first().click(); assert(await native.getByRole('link', { name: 'Latest stories', exact: true }).isVisible()); } finally { await noScript.close(); }
    report.status = 'passed';
  } finally {
    if(report.status!=='passed'){ report.ui_status=await admin.locator('[role=status]').allTextContents(); await admin.screenshot({path:path.join(output,'d04-studio-failure.png'),fullPage:true}); fs.writeFileSync(path.join(output,'d04-studio-failure.html'),await admin.content()); }
    if (previous && csrf) await owner.request.post(origin + '/api/admin/design/' + previous + '/activate', { headers: { Origin: origin }, data: { csrf } });
    fs.writeFileSync(path.join(output, 'd04-theme-assets.json'), JSON.stringify(report, null, 2));
    await admin.close(); await page.close();
  }
};
