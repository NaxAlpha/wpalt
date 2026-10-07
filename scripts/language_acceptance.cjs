// Native draft duplication, saved comparison and actual LTR/RTL measurements.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const ui=require('./ui_contracts.cjs');
module.exports=async(owner,publicContext,origin,output)=>{
 const page=await owner.newPage();let nativeContext;
 const measurements=[],accessibility=[],errors=[],remote=[];
 const submit=async(p,b)=>{const [r]=await Promise.all([p.waitForResponse(r=>r.request().method()==='POST'&&new URL(r.url()).pathname.startsWith('/admin/')),p.waitForNavigation({waitUntil:'load'}),b.click()]);if(r.status()>=400)throw new Error(`Language operation failed (${r.status()}): ${await r.text()}`);};
 try{
  await page.goto(origin+'/admin/discovery');
  const configuration=page.locator('form[action="/admin/discovery"]');
  const definition=JSON.parse(await configuration.locator('textarea[name=definition]').inputValue());
  for(const language of [{code:'fr',label:'Français',direction:'ltr',navigation:[],search_label:'Rechercher'},{code:'ja',label:'日本語',direction:'ltr',navigation:[],search_label:'検索'},{code:'ar',label:'العربية',direction:'rtl',navigation:[],search_label:'بحث'}]){
   if(!definition.languages.some(l=>l.code===language.code))definition.languages.push(language);
  }
  const response=await owner.request.post(origin+'/admin/discovery',{headers:{Origin:origin},form:{csrf:await configuration.locator('input[name=csrf]').inputValue(),version:await configuration.locator('input[name=version]').inputValue(),definition:JSON.stringify(definition)}});assert(response.status()<400,await response.text());
  nativeContext=await owner.browser().newContext({javaScriptEnabled:false,storageState:await owner.storageState(),viewport:{width:320,height:1000}});
  nativeContext.on('request',r=>{if(!r.url().startsWith(origin))remote.push(r.url());});
  const native=await nativeContext.newPage();native.on('pageerror',e=>errors.push(e.message));
  await native.goto(origin+'/admin/posts/new');
  await native.getByLabel('Title',{exact:true}).fill('A connected language story');
  await native.getByLabel('URL slug',{exact:true}).fill('connected-language-story');
  await native.getByRole('textbox',{name:'Content',exact:true}).fill('A saved language source with mixed identifiers: العربية 日本語 ABC-123.\n\n'+('A calm garden and deliberate local publishing. '.repeat(90)));
  await native.getByText('Language & discovery',{exact:true}).click();
  await native.getByLabel('Translation group',{exact:true}).fill('browser-language-family');
  await submit(native,native.getByRole('button',{name:'Save draft',exact:true}));
  const source=native.url().split('/').pop();
  for(const locale of ['fr','ja','ar']){
   await native.goto(origin+'/admin/languages/'+source);
   await native.getByLabel('Target language',{exact:true}).selectOption(locale);
   await native.getByLabel('Target URL slug',{exact:true}).fill('connected-language-'+locale);
   await submit(native,native.getByRole('button',{name:'Preview language operation',exact:true}));
   await native.getByText('Review this exact saved source, target and selection. Changes before execution require a new preview.',{exact:true}).waitFor();
   await submit(native,native.getByRole('button',{name:'Create reviewed draft',exact:true}));
   const target=native.url().split('/').pop();assert.notEqual(target,source);
   assert.equal((await publicContext.request.get(origin+'/'+locale+'/connected-language-'+locale)).status(),404);
   await page.goto(origin+'/admin/languages/'+source+'?target='+target);
   assert.equal(await page.locator(`section[lang="${locale}"]`).getAttribute('dir'),locale==='ar'?'rtl':'ltr');
   await page.getByText('A calm garden and deliberate local publishing.',{exact:false}).first().waitFor();
   for(const width of [320,768,1440]){await page.setViewportSize({width,height:1000});const m=await ui.geometry(page);assert.deepEqual(m.failures,[],`Language comparison ${locale}/${width}`);measurements.push({surface:'saved comparison',locale,...m});await page.screenshot({path:path.join(output,`d03-comparison-${locale}-${width}.png`),fullPage:true});}
   if(locale==='ar'){
    await page.route(origin+'/__ui_fixture/axe.js',r=>r.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(__dirname,'../frontend/node_modules/axe-core/axe.min.js'))}));
    await ui.accessibility(page,origin,'mixed-direction saved language comparison',{accessibility});
   }
  }
  await page.goto(origin+'/admin/languages');await page.getByRole('heading',{name:'Language workspace',exact:true}).waitFor();
  assert.deepEqual(errors,[]);assert.deepEqual(remote,[]);
  fs.writeFileSync(path.join(output,'d03-language-measurements.json'),JSON.stringify({browser:owner.browser().version(),measurements,accessibility,errors,remote,scope:'Saved content comparison and native draft duplication. Account interface localization is not implemented yet.'},null,2));
  console.log('PASS: native language preview/draft duplication, private French/Japanese/Arabic variants, mixed-direction saved comparison, reflow and scoped accessibility');
 }finally{if(nativeContext)await nativeContext.close();await page.close();}
};
