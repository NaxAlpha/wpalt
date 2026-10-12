// Execute the actual free English research bundle, with a connected native source.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

module.exports = async (owner, visitors, origin, output, id, csrf) => {
  const root = path.resolve(process.env.WPALT_EDITORIAL_REFERENCE_SOURCE || 'work/d09-reference-source');
  const pins = JSON.parse(fs.readFileSync(path.join(root, 'pins.json')));
  const htmlResponse = await visitors.request.get(origin + '/editorial-actual-copy');
  assert(htmlResponse.ok());
  const html = await htmlResponse.text();
  const response = await owner.request.post(origin + '/api/admin/insights/' + id, {
    headers: { Origin: origin }, data: { csrf, copy: 'public', focus: 'café' },
  });
  assert(response.ok());
  const native = await response.json();
  assert.equal(native.html_sha256, crypto.createHash('sha256').update(html).digest('hex'));
  assert(!native.text.body.includes('PRIVATE WORKING'));
  const context = await owner.browser().newContext();
  const page = await context.newPage();
  const errors = [], requests = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.route('**/*', r => { requests.push(r.request().url()); return r.abort(); });
  try {
    for (const filename of ['wp-lodash.js', 'wp-hooks.js', 'wp-i18n.js', 'yoast-feature.js', 'yoast-analysis.js', 'yoast-researcher.js']) {
      const bundle = fs.readFileSync(path.join(root, filename));
      assert.equal(crypto.createHash('sha256').update(bundle).digest('hex'), pins.bundles[filename].sha256);
      await page.addScriptTag({ content: bundle.toString('utf8') });
      // The bootstrap is the actual WordPress script-loader lodash convention.
      if (filename === 'wp-lodash.js') await page.evaluate(() => { window.lodash = window._.noConflict(); });
    }
    const observed = await page.evaluate(({ html, native }) => {
      const a = window.yoast.analysis;
      const Researcher = window.yoast.Researcher.default || window.yoast.Researcher;
      const research = (body, title, description, phrase) => {
        const paper = new a.Paper(body, { keyword: phrase, title, description, locale: 'en_US' });
        const researcher = new Researcher(paper);
        a.ensureTree(paper, researcher);
        return {
          words: researcher.getResearch('wordCountInText'),
          phrase: researcher.getResearch('getKeyphraseCount').count,
          title: researcher.getResearch('findKeyphraseInSEOTitle'),
          description: researcher.getResearch('metaDescriptionKeyword'),
          anchors: researcher.getResearch('getAnchorsWithKeyphrase').anchorsWithKeyphraseCount,
        };
      };
      const main = new DOMParser().parseFromString(html, 'text/html').querySelector('main');
      if (!main) throw Error('Actual native source must declare main');
      return {
        controlled: research('<p>A café guide. A cat, cathedral and education.</p>', 'Café guide', 'Café notes', 'café'),
        connected: research(main.outerHTML, native.text.title, native.text.description, 'café'),
      };
    }, { html, native });
    assert.equal(observed.controlled.words.count, 8);
    assert.equal(observed.controlled.phrase, 1);
    assert(observed.controlled.title.exactMatchFound);
    assert.equal(observed.controlled.description, 1);
    assert.equal(observed.connected.phrase, 3);
    const mainPhrase = native.observations.locations.find(l => l.label === 'main text').occurrences;
    assert.equal(mainPhrase, 3);
    assert.deepEqual(errors, []);
    assert.deepEqual(requests, []);
    const receipt = {
      status: 'passed', pins, browser: owner.browser().version(),
      execution: 'Actual pinned free Yoast analysis/English researcher plus actual WordPress dependency bundles in Chrome; no requests allowed. This is component execution, not a WordPress PHP/admin workflow or premium assessment comparison.',
      controlled: observed.controlled, connected: observed.connected,
      native: { html_sha256: native.html_sha256, html_bytes: native.html_bytes, observations: native.observations },
      differences: 'The shared fixture verifies literal café occurrences. Yoast English analysis and wpalt default Unicode segmentation have different language/boundary/markup scopes; word counts are recorded independently, with no universal score or complete parity inference.',
      errors, requests,
    };
    fs.writeFileSync(path.join(output, 'd09-yoast-reference.json'), JSON.stringify(receipt, null, 2) + '\n');
  } finally { await context.close(); }
};
