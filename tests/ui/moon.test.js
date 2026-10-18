// The moon launcher: the surface layer keeps turning and the page renders without errors.
const puppeteer = require('puppeteer-core');
const fs = require('fs');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const OUT = path.join(__dirname, 'out');
fs.mkdirSync(OUT, { recursive: true });

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true, args: ['--allow-file-access-from-files'] });
  const page = await browser.newPage();
  await page.setViewport({ width: 72, height: 72, deviceScaleFactor: 4 });
  await page.evaluateOnNewDocument(`window.__TAURI__ = {
    core: { invoke: async c => c === 'get_settings' ? { moon_edge: 'left' } : 'left' },
    event: { listen: async () => () => {} },
    window: { getCurrentWindow: () => ({}) },
  };`);
  page.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await page.goto('file:///' + UI + '/moon.html');
  await page.evaluate(() => { document.documentElement.style.background = '#e9edf5'; });
  await page.waitForFunction(() => [...document.querySelectorAll('.orb img')].every(i => i.complete && i.naturalWidth));

  const angle = () => page.evaluate(() => getComputedStyle(document.querySelector('.surface')).transform);
  const a0 = await angle();
  await page.screenshot({ path: path.join(OUT, 'moon_a.png') });
  await new Promise(r => setTimeout(r, 2500));
  const a1 = await angle();
  await page.screenshot({ path: path.join(OUT, 'moon_b.png') });
  check('surface keeps turning', a0 !== a1, `${a0} -> ${a1}`);
  check('all three layers loaded', await page.evaluate(() => document.querySelectorAll('.orb img').length === 3));

  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  await browser.close();
  process.exit(failures ? 1 : 0);
})();
