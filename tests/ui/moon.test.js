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
    core: { invoke: async c => c === 'get_settings' ? { moon_edge: 'left', moon_speed: 'fast' } : 'left' },
    event: { listen: async (ev, cb) => { (window.__h = window.__h || {})[ev] = cb; return () => {}; } },
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
  const turn = () => page.evaluate(() => [getComputedStyle(document.querySelector('.surface')).animationDuration, getComputedStyle(document.querySelector('.surface')).animationName]);
  check('speed comes from Settings', (await turn())[0] === '35s', JSON.stringify(await turn()));
  await page.evaluate(() => window.__h['moon-speed']({ payload: 'still' }));
  check('Still stops the moon', (await turn())[1] === 'none', JSON.stringify(await turn()));
  await page.evaluate(() => window.__h['moon-speed']({ payload: 'normal' }));
  check('Normal is 75 s per turn', JSON.stringify(await turn()) === JSON.stringify(['75s', 'turn']), JSON.stringify(await turn()));
  check('all three layers loaded', await page.evaluate(() => document.querySelectorAll('.orb img').length === 3));
  check('moon is the default body', await page.evaluate(() => !document.body.classList.contains('planet') && document.body.classList.contains('moon')));

  // Every other body is painted on the canvas and keeps turning.
  const pixels = () => page.evaluate(() => { const c = document.querySelector('.globe'); return c.toDataURL(); });
  for (const id of ['sun', 'mercury', 'venus', 'earth', 'mars', 'jupiter', 'saturn', 'uranus', 'neptune', 'pluto']) {
    await page.evaluate(id => window.__h['moon-body']({ payload: id }), id);
    const shown = await page.evaluate(() => getComputedStyle(document.querySelector('.globe')).display !== 'none' && getComputedStyle(document.querySelector('.orb img')).display === 'none');
    const p0 = await pixels();
    await new Promise(r => setTimeout(r, 900));
    const p1 = await pixels();
    check(`${id} is drawn and turns`, shown && p0 !== p1 && p0.length > 2000);
    await page.screenshot({ path: path.join(OUT, `moon_${id}.png`) });
  }
  check('hide label names the body', await page.evaluate(() => document.getElementById('hideLbl').textContent === 'Hide Pluto'));
  await page.evaluate(() => window.__h['moon-speed']({ payload: 'still' }));
  const s0 = await pixels(); await new Promise(r => setTimeout(r, 600));
  check('Still stops a planet too', s0 === await pixels());
  await page.evaluate(() => window.__h['moon-body']({ payload: 'moon' }));
  check('back to the moon', await page.evaluate(() => getComputedStyle(document.querySelector('.globe')).display === 'none'));

  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  await browser.close();
  process.exit(failures ? 1 : 0);
})();
