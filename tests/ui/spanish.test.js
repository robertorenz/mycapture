// Every window in Spanish: no text that has a translation may still show in English,
// and changing the language in Settings is remembered for all windows.
const puppeteer = require('puppeteer-core');
const fs = require('fs');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const PNG = fs.readFileSync(path.join(__dirname, 'fixture.png'));

const mock = (kind, lang) => `
  try { if (!sessionStorage.getItem('seeded')) { localStorage.setItem('mycapture.lang', '${lang}'); sessionStorage.setItem('seeded', '1'); } } catch {}
  window.__calls = [];
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => {
      window.__calls.push([cmd, args]);
      if (cmd === 'history_get') return { id: 'x', kind: '${kind}', file: '${kind === 'image' ? 'test.png' : 'clip.mp4'}', thumb: 't.jpg', created: new Date().toISOString(), width: 800, height: 500, duration: 12, url: null };
      if (cmd === 'history_list') return [{ id: 'a', kind: 'image', file: 'a.png', thumb: 'a.jpg', created: new Date().toISOString(), width: 10, height: 10, duration: 0, url: 'https://x' }];
      if (cmd === 'get_settings') return JSON.parse(localStorage.getItem('test.settings') || 'null') || { language: '${lang}', hotkey: 'CommandOrControl+Shift+J', moon_edge: 'top', camera_size: 220, fps: 30, upload: { provider: 'dropbox' } };
      if (cmd === 'set_settings') { localStorage.setItem('test.settings', JSON.stringify(args.settings)); return null; }
      if (cmd === 'share_targets') return [{ id: 'dropbox', label: 'Dropbox', detail: 'a@b.c', default: true }];
      if (cmd === 'cloud_status') return [{ id: 'dropbox', account: null, builtin: false }, { id: 'onedrive', account: 'me@outlook.com', builtin: true }, { id: 'gdrive', account: null, builtin: false }];
      if (cmd === 'overlay_info') return { width: 800, height: 500, windows: [] };
      if (cmd === 'record_status') return { active: true, paused: false, elapsed: 3, countdown: 0, mic: true, desktop_audio: true, camera: false, converts: false };
      if (cmd === 'list_mics' || cmd === 'list_codecs') return [];
      if (cmd === 'check_ffmpeg') return 'ffmpeg';
      if (cmd === 'scroll_info') return { auto_supported: true };
      return null;
    } },
    app: { getVersion: async () => '1.0.0' },
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'overlay-0', close() {}, show: async () => {}, startDragging() {} }) },
  };
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function open(browser, page, kind = 'image', lang = 'es', w = 1100, h = 760) {
  const p = await browser.newPage();
  await p.setViewport({ width: w, height: h });
  p.on('pageerror', e => { console.log(page, 'PAGE ERROR', e.message); failures++; });
  await p.setRequestInterception(true);
  p.on('request', r => r.url().includes('mycap.localhost') ? r.respond({ status: 200, contentType: 'image/png', body: PNG }) : r.continue());
  await p.evaluateOnNewDocument(mock(kind, lang));
  await p.goto('file:///' + UI + '/' + page);
  await sleep(700);
  return p;
}

// Text on screen that equals an English key with a different Spanish translation.
const leftovers = p => p.evaluate(() => {
  const english = new Set(Object.entries(STRINGS_ES).filter(([en, es]) => en !== es && !en.startsWith('html:')).map(([en]) => en));
  const found = new Set();
  const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n; (n = w.nextNode());) {
    const t = n.nodeValue.trim();
    if (t && english.has(t) && !n.parentElement.closest('script, style, [data-no-tr]')) found.add(t);
  }
  document.querySelectorAll('[title], [placeholder], [aria-label]').forEach(e => ['title', 'placeholder', 'aria-label'].forEach(a => english.has(e.getAttribute(a)) && found.add(`@${a}: ${e.getAttribute(a)}`)));
  if (english.has(document.title)) found.add('@title: ' + document.title);
  return [...found];
});

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true, args: ['--allow-file-access-from-files'] });

  const pages = [['editor.html?id=x'], ['history.html'], ['overlay.html?i=0'], ['recbar.html', 'image', 'es', 400, 56], ['moon.html', 'image', 'es', 300, 200], ['notice.html?msg=x', 'image', 'es', 460, 260], ['camera.html', 'image', 'es', 220, 220], ['scroll.html', 'image', 'es', 470, 56], ['settings.html']];
  for (const args of pages) {
    const p = await open(browser, ...args);
    if (args[0] === 'settings.html') {
      for (const sec of ['recording', 'sharing', 'about']) await p.evaluate(s => document.querySelector(`nav a[data-sec="${s}"]`).click(), sec);
      await p.evaluate(() => document.querySelectorAll('details').forEach(d => d.open = true));
    }
    const left = await leftovers(p);
    check(`${args[0]} has no English left`, left.length === 0, JSON.stringify(left));
    check(`${args[0]} is marked Spanish`, await p.evaluate(() => document.documentElement.lang === 'es'));
    await p.close();
  }

  // Settings: English UI, pick Español, save → remembered for every window and redrawn in Spanish.
  const p = await open(browser, 'settings.html', 'image', 'en');
  check('English by default', await p.$eval('nav a[data-sec="sharing"]', e => e.textContent.trim()) === 'Sharing');
  await p.select('#language', 'es');
  await p.click('#save');
  await p.waitForFunction(() => document.querySelector('nav a[data-sec="sharing"]')?.textContent.trim() === 'Compartir', { timeout: 5000 });
  check('saving Español redraws Settings in Spanish', true);
  check('language remembered for other windows', await p.evaluate(() => localStorage.getItem('mycapture.lang')) === 'es');
  await p.screenshot({ path: path.join(__dirname, 'out', 'es-settings.png') });
  await p.close();

  await browser.close();
  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  process.exit(failures ? 1 : 0);
})();
