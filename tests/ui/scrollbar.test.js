// Scrolling-capture bar: shows the stitched height, warns when scrolled too fast,
// auto-scroll toggles, and it finishes by itself when auto-scroll reaches the end.
const puppeteer = require('puppeteer-core');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const mock = `
  window.__calls = [];
  window.__handlers = {};
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      window.__calls.push([cmd, args]);
      if (cmd === 'scroll_info') return { auto_supported: true };
      return null;
    } },
    event: { listen: async (ev, cb) => { window.__handlers[ev] = cb; return () => {}; }, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'scrollbar', close() {}, startDragging() {} }) },
  };
  window.__fire = (ev, payload) => window.__handlers[ev]?.({ payload });
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };
const sleep = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 470, height: 56 });
  page.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await page.evaluateOnNewDocument(mock);
  await page.goto('file:///' + UI + '/scroll.html');
  await sleep(300);
  const text = () => page.evaluate(() => ({ msg: document.getElementById('msg').textContent, size: document.getElementById('size').textContent, cls: document.body.className }));
  const calls = () => page.evaluate(() => window.__calls.map(c => c[0] + (c[1]?.on !== undefined ? ':' + c[1].on : '')));

  check('auto-scroll offered where supported', await page.$eval('#auto', e => e.style.display !== 'none'));
  await page.evaluate(() => window.__fire('scroll-progress', { height: 1840, state: 'ok' }));
  let t = await text();
  check('shows stitched height', t.size === '1,840 px' && t.msg === 'Scroll down slowly', JSON.stringify(t));
  await page.evaluate(() => window.__fire('scroll-progress', { height: 1840, state: 'lost' }));
  t = await text();
  check('warns when scrolled too fast', t.cls === 'lost' && t.msg.startsWith('Too fast'), JSON.stringify(t));

  await page.click('#auto');
  await sleep(50);
  check('auto-scroll turns on', (await calls()).includes('scroll_auto:true') && (await page.$eval('#auto', e => e.classList.contains('on'))));
  await page.evaluate(() => window.__fire('scroll-progress', { height: 5200, state: 'end' }));
  await sleep(50);
  check('reaching the end finishes by itself', (await calls()).includes('scroll_finish'), JSON.stringify(await calls()));
  await page.screenshot({ path: path.join(__dirname, 'out', 'scrollbar.png') });

  await browser.close();
  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  process.exit(failures ? 1 : 0);
})();
