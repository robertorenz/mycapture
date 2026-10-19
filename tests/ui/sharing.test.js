// Settings → Sharing: the cloud drive cards, setup steps and connect / disconnect flow.
const puppeteer = require('puppeteer-core');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const mock = `
  window.__calls = [];
  window.__cloud = [
    { id: 'dropbox', account: null, builtin: false },
    { id: 'onedrive', account: 'me@outlook.com', builtin: true },
    { id: 'gdrive', account: null, builtin: true },
  ];
  const settings = { hotkey: 'CommandOrControl+Shift+J', fps: 30, upload: { provider: 'dropbox', dropbox_app_key: '' } };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      window.__calls.push([cmd, args]);
      if (cmd === 'get_settings') return JSON.parse(JSON.stringify(settings));
      if (cmd === 'cloud_status') return JSON.parse(JSON.stringify(window.__cloud));
      if (cmd === 'cloud_connect') { window.__cloud[0].account = 'me@dropbox.com'; return 'me@dropbox.com'; }
      if (cmd === 'cloud_disconnect') { window.__cloud[1].account = null; return null; }
      if (cmd === 'list_mics' || cmd === 'list_codecs') return [];
      if (cmd === 'check_ffmpeg') return 'ffmpeg';
      return null;
    } },
    app: { getVersion: async () => '9.9.9' },
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'settings', close() {} }) },
  };
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };
const sleep = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 900, height: 760 });
  page.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await page.evaluateOnNewDocument(mock);
  await page.goto('file:///' + UI + '/settings.html');
  await sleep(400);
  await page.evaluate(() => document.querySelector('nav a[data-sec="sharing"]').click());

  const state = () => page.evaluate(() => ({
    pane: document.querySelector('.pane.on')?.dataset.pane,
    acct: document.querySelector('.pane.on .acct')?.textContent,
    btn: document.querySelector('.pane.on .conn')?.textContent,
    ownOpen: document.querySelector('.pane.on .ownapp')?.open,
    noteShown: getComputedStyle(document.querySelector('.pane.on .builtin-note')).display !== 'none',
  }));

  let s = await state();
  check('Dropbox pane shown for the saved provider', s.pane === 'dropbox', JSON.stringify(s));
  check('no built-in key: setup steps open', s.ownOpen && s.noteShown, JSON.stringify(s));
  check('version comes from the app', await page.$eval('#version', e => e.textContent) === '9.9.9');

  await page.click('.pane.on .conn');
  await sleep(200);
  s = await state();
  const calls = await page.evaluate(() => window.__calls.map(c => c[0]));
  check('Connect saves settings first, then signs in', calls.indexOf('set_settings') > -1 && calls.indexOf('set_settings') < calls.indexOf('cloud_connect'), calls.join(','));
  check('account shown after connecting', s.acct === 'Connected as me@dropbox.com' && s.btn === 'Disconnect', JSON.stringify(s));

  await page.click('.prov[data-p="onedrive"]');
  s = await state();
  check('OneDrive with built-in key hides setup note', s.pane === 'onedrive' && !s.ownOpen && !s.noteShown && s.acct.includes('me@outlook.com'), JSON.stringify(s));

  await page.click('.pane.on .conn');
  await page.waitForSelector('.modal');
  check('Disconnect asks in a modal', true);
  await page.evaluate(() => document.querySelector('.modal .actions .primary').click());
  await sleep(200);
  s = await state();
  check('disconnected', s.acct === 'Not connected' && s.btn === 'Connect', JSON.stringify(s));

  // Picking a card only opens it; the default changes with Make default.
  await page.click('.prov[data-p="gdrive"]');
  check('opening a card keeps the default', await page.$eval('.prov[data-p="dropbox"]', e => e.classList.contains('is-def'))
    && !(await page.$eval('#makeDef', e => e.disabled)), await page.$eval('#defNote', e => e.textContent));
  await page.click('#makeDef');
  await sleep(150);
  const saved = await page.evaluate(() => window.__calls.filter(c => c[0] === 'set_settings').pop()[1].settings.upload.provider);
  check('Make default saves it', saved === 'gdrive' && await page.$eval('.prov[data-p="gdrive"]', e => e.classList.contains('is-def')), saved);

  const cols = await page.$$eval('.prov', els => els.length);
  check('seven sharing choices', cols === 7, cols);
  await page.screenshot({ path: path.join(__dirname, 'out', 'sharing.png') });

  await browser.close();
  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  process.exit(failures ? 1 : 0);
})();
