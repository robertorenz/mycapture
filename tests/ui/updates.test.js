// Settings → About: update check, release notes, download progress and install.
const puppeteer = require('puppeteer-core');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const mock = update => `
  window.__calls = [];
  window.__handlers = {};
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      window.__calls.push(cmd);
      if (cmd === 'get_settings') return { hotkey: 'CommandOrControl+Shift+J', fps: 30, check_updates: true, upload: { provider: 'none' } };
      if (cmd === 'update_check') return ${JSON.stringify(update)};
      if (cmd === 'update_install') return new Promise(() => {});
      if (cmd === 'cloud_status') return [];
      if (cmd === 'list_mics' || cmd === 'list_codecs') return [];
      if (cmd === 'check_ffmpeg') return 'ffmpeg';
      return null;
    } },
    app: { getVersion: async () => '0.3.0' },
    event: { listen: async (ev, cb) => { window.__handlers[ev] = cb; return () => {}; }, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'settings', close() {} }) },
  };
  window.__fire = (ev, payload) => window.__handlers[ev]?.({ payload });
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function open(browser, update) {
  const p = await browser.newPage();
  await p.setViewport({ width: 900, height: 760 });
  p.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await p.evaluateOnNewDocument(mock(update));
  await p.goto('file:///' + UI + '/settings.html#about');
  await sleep(400);
  return p;
}

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });

  let p = await open(browser, { current: '0.3.0', version: '0.3.1', notes: '- Faster stitching\n- Spanish fixes', date: '2026-10-20' });
  check('opens on About when asked', await p.$eval('#about', e => e.classList.contains('on')));
  check('About shows a dot when an update is waiting', !!(await p.$('nav a[data-sec="about"] .badge-dot')));
  check('update card shows version and notes',
    (await p.$eval('#updStatus', e => e.textContent)).includes('0.3.1') && (await p.$eval('#updNotes', e => e.textContent)).includes('Faster stitching')
    && await p.$eval('#upd', e => e.classList.contains('has-new')));
  await p.click('#updInstall');
  await sleep(50);
  check('install starts the download', (await p.evaluate(() => window.__calls)).includes('update_install') && await p.$eval('#updInstall', e => e.disabled));
  await p.evaluate(() => window.__fire('update-progress', { downloaded: 26214400, total: 52428800 }));
  check('progress shows MB and half a bar', (await p.$eval('#updHint', e => e.textContent)).includes('25.0 MB / 50.0 MB')
    && (await p.$eval('#updMeter i', e => e.style.width)) === '50%');
  await p.screenshot({ path: path.join(__dirname, 'out', 'updates.png') });
  await p.close();

  p = await open(browser, { current: '0.3.0', version: null, notes: null, date: null });
  check('no update: no dot, no install button', !(await p.$('nav a[data-sec="about"] .badge-dot')) && !(await p.$eval('#upd', e => e.classList.contains('has-new'))));
  await p.click('#updCheck');
  await sleep(100);
  check('Check for updates says up to date', (await p.$eval('#updStatus', e => e.textContent)) === "You're up to date.");
  await p.close();

  await browser.close();
  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  process.exit(failures ? 1 : 0);
})();
