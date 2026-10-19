// The split Share button: main half uses the default destination, the arrow picks any set-up one.
const puppeteer = require('puppeteer-core');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const mock = `
  window.__calls = [];
  window.__targets = [
    { id: 'dropbox', label: 'Dropbox', detail: 'me@dropbox.com', default: true },
    { id: 'gdrive', label: 'Google Drive', detail: 'me@gmail.com', default: false },
    { id: 's3', label: 'S3 storage', detail: 'my-bucket', default: false },
  ];
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      window.__calls.push([cmd, args]);
      if (cmd === 'get_settings') return { hotkey: 'CommandOrControl+Shift+J' };
      if (cmd === 'history_list') return [{ id: 'a', kind: 'image', file: 'a.png', thumb: 'a.jpg', created: new Date().toISOString(), width: 10, height: 10, duration: 0, url: null }];
      if (cmd === 'share_targets') return JSON.parse(JSON.stringify(window.__targets));
      if (cmd === 'set_share_default') { window.__targets.forEach(t => t.default = t.id === args.provider); return null; }
      if (cmd === 'share') return 'https://example.com/' + (args.provider || 'default');
      return null;
    } },
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'history', close() {} }) },
  };
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };
const sleep = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 900, height: 600 });
  page.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await page.evaluateOnNewDocument(mock);
  await page.goto('file:///' + UI + '/history.html');
  await page.waitForSelector('.split');
  const lastShare = () => page.evaluate(() => window.__calls.filter(c => c[0] === 'share').pop());

  await page.click('.split-main');
  await sleep(100);
  let call = await lastShare();
  check('main half shares to the default', call && call[1].id === 'a' && call[1].provider === null, JSON.stringify(call));
  check('card did not open the editor', !(await page.evaluate(() => window.__calls.some(c => c[0] === 'open_editor'))));

  await page.click('.split-more');
  await page.waitForSelector('.menu');
  const rows = await page.$$eval('.menu .menu-row', els => els.map(e => e.textContent.replace(/\s+/g, ' ').trim()));
  check('menu lists every set-up destination', rows.length === 3 && rows[1].includes('Google Drive') && rows[1].includes('me@gmail.com'), JSON.stringify(rows));
  check('default is starred', await page.$eval('.menu-star[data-star="dropbox"]', e => e.classList.contains('on')));
  const box = await page.$eval('.menu', e => { const r = e.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; });
  check('menu stays inside the window', box[0] >= 0 && box[1] >= 0 && box[2] <= 900 && box[3] <= 600, JSON.stringify(box));

  await page.click('.menu-item[data-p="gdrive"]');
  await sleep(100);
  call = await lastShare();
  check('picking a destination shares there', call[1].provider === 'gdrive', JSON.stringify(call));
  check('menu closes after picking', !(await page.$('.menu')));

  await page.click('.split-more');
  await page.waitForSelector('.menu');
  await page.click('.menu-star[data-star="s3"]');
  await sleep(150);
  check('star makes it the default', await page.$eval('.menu-star[data-star="s3"]', e => e.classList.contains('on'))
    && !(await page.$eval('.menu-star[data-star="dropbox"]', e => e.classList.contains('on'))));
  await page.screenshot({ path: path.join(__dirname, 'out', 'sharemenu.png') });
  await page.keyboard.press('Escape');
  check('Escape closes the menu', !(await page.$('.menu')));

  await page.evaluate(() => { window.__targets.forEach(t => t.default = false); });
  await page.click('.split-main');
  await page.waitForSelector('.menu');
  check('with no default, main half opens the menu', true);
  await page.click('.menu-item[data-setup]');
  await sleep(50);
  check('Set up sharing opens Settings on Sharing', await page.evaluate(() => window.__calls.some(c => c[0] === 'open_settings' && c[1]?.section === 'sharing')));

  await browser.close();
  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  process.exit(failures ? 1 : 0);
})();
