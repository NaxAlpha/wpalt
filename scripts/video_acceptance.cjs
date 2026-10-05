// Real native worker + browser preview and retry; no remote codec or service.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ui = require("./ui_contracts.cjs");
module.exports = async (owner, anonymous, origin, output, fixture) => {
  const page = await owner.newPage();
  const report = {journey:[],measurements:[],accessibility:[],browser:owner.browser().version(),ffmpeg:process.env.WPALT_VIDEO_TOOL_VERSION};
  const errors = [];
  page.on("pageerror",error=>errors.push(error.message));
  await page.route(origin+"/__ui_fixture/axe.js",route=>route.fulfill({contentType:"text/javascript",body:fs.readFileSync(path.join(__dirname,"../frontend/node_modules/axe-core/axe.min.js"))}));
  await page.route(origin+"/__ui_fixture/video-spacing.css",route=>route.fulfill({contentType:"text/css",body:"body *{line-height:1.5!important;letter-spacing:.12em!important;word-spacing:.16em!important}p{margin-bottom:2em!important}"}));
  const measure = async name => {
    for(const width of [320,768,1440]) {
      await page.setViewportSize({width,height:1000});
      const normal = await ui.geometry(page);
      assert.deepEqual(normal.failures,[],`${name} at ${width}`);
      report.measurements.push({name,state:"normal",...normal});
      await page.screenshot({path:path.join(output,`${name}-${width}.png`),fullPage:true});
      const style = await page.addStyleTag({url:origin+"/__ui_fixture/video-spacing.css"});
      const spaced = await ui.geometry(page);
      assert.deepEqual(spaced.failures,[],`${name} spacing at ${width}`);
      report.measurements.push({name,state:"text-spacing",...spaced});
      await style.evaluate(node=>node.remove());
    }
    await ui.accessibility(page,origin,name,report);
  };
  let release;
  try {
    await page.goto(origin+"/admin/media");
    const form = page.locator("form[data-local-video]");
    await form.getByLabel("Video source",{exact:true}).setInputFiles({name:"damaged.mp4",mimeType:"video/mp4",buffer:Buffer.from("not a video")});
    await form.getByRole("button",{name:"Process local video",exact:true}).click();
    await form.getByRole("alert").waitFor();
    assert.equal(await form.getByRole("button",{name:"Process local video",exact:true}).isEnabled(),true);
    assert.equal(await form.getByLabel("Video source",{exact:true}).evaluate(node=>node.files[0].name),"damaged.mp4","failed processing retains owner source choice");
    assert.equal(await page.locator(".media-card video").count(),0);
    await measure("video-retry-error");
    report.journey.push("Damaged source creates no inventory; inline retry retains selected source");

    await form.getByLabel("Video source",{exact:true}).setInputFiles(fixture);
    const held = new Promise(resolve=>{release=resolve;});
    await page.route(origin+"/admin/media/video",async route=>{await held;await route.continue();});
    await form.getByRole("button",{name:"Process local video",exact:true}).click();
    await form.getByText(/Processing locally/).waitFor();
    assert.equal(await form.getByRole("button",{name:"Process local video",exact:true}).isDisabled(),true);
    assert.equal(await form.getAttribute("aria-busy"),"true");
    await measure("video-processing-busy");
    const navigated = page.waitForNavigation({waitUntil:"load"});
    release();
    await navigated;
    await page.unroute(origin+"/admin/media/video");
    const card = page.locator(".media-card").filter({has:page.locator("video")});
    assert.equal(await card.count(),1,"one processed result");
    const video = card.locator("video");
    await page.waitForFunction(()=>document.querySelector(".media-card video")?.readyState>=1);
    const metadata = await video.evaluate(node=>({width:node.videoWidth,height:node.videoHeight,duration:node.duration,muted:node.muted}));
    assert(metadata.width>0 && metadata.height>0 && metadata.width<=1280 && metadata.height<=720);
    assert(metadata.duration>0 && metadata.duration<=2 && metadata.muted);
    await video.evaluate(node=>node.play());
    await page.waitForFunction(()=>document.querySelector(".media-card video").currentTime>0);
    await video.evaluate(node=>node.pause());
    const source = await video.locator("source").getAttribute("src");
    assert.equal((await anonymous.request.get(origin+source)).status(),401);
    const picker = await (await owner.request.get(origin+"/api/admin/media")).json();
    assert(!picker.items.some(item=>source.endsWith(item.id)),"processed video stays out of image pickers");
    await measure("video-private-library");
    report.journey.push("Actual MP4 decodes/plays quietly; private authority and image-picker isolation hold");
    await card.getByLabel("Visibility",{exact:true}).selectOption("public");
    await Promise.all([page.waitForNavigation(),card.getByRole("button",{name:"Save details",exact:true}).click()]);
    assert.equal((await anonymous.request.get(origin+source)).status(),200);
    const changed = page.locator(".media-card").filter({has:page.locator(`source[src="${source}"]`)});
    await changed.getByLabel("Visibility",{exact:true}).selectOption("private");
    await Promise.all([page.waitForNavigation(),changed.getByRole("button",{name:"Save details",exact:true}).click()]);
    assert.equal((await anonymous.request.get(origin+source)).status(),401,"revocation applies after public delivery");
    report.journey.push("Owner publication and later access revocation preserve the same media identity");
    assert.deepEqual(errors,[]);
    report.metadata=metadata;report.status="passed";
  } finally {
    if(release)release();
    await page.close();
    fs.writeFileSync(path.join(output,"video-result.json"),JSON.stringify(report,null,2));
  }
};
