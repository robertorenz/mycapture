// The preview window must keep the video's player controls on screen and let it play.
const puppeteer = require('puppeteer-core');
const fs = require('fs');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const MP4 = process.env.TEST_MP4;
if (!MP4 || !fs.existsSync(MP4)) { console.log('Set TEST_MP4 to a recording to run this test; skipping.'); process.exit(0); }
const VIDEO = fs.readFileSync(MP4);

const mock = `
  window.__TAURI__ = {
    core: { invoke: async (cmd) => cmd === 'history_get'
      ? { id: 'v', kind: 'video', file: 'clip.mp4', thumb: 't.jpg', created: new Date().toISOString(), width: 1102, height: 690, duration: 12, url: null }
      : null },
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'ed_v', close() {} }) },
  };
  Object.defineProperty(navigator, 'userAgent', { get: () => 'Mozilla/5.0 (Windows NT 10.0)' });
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true, args: ['--allow-file-access-from-files', '--autoplay-policy=no-user-gesture-required'] });
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 760 });
  page.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await page.setRequestInterception(true);
  page.on('request', r => {
    if (!r.url().startsWith('http://mycap.localhost/file/')) return r.continue();
    // Serve byte ranges like the app's protocol handler does.
    const m = /bytes=(\d+)-(\d*)/.exec(r.headers().range || '');
    if (!m) return r.respond({ status: 200, contentType: 'video/mp4', headers: { 'Accept-Ranges': 'bytes' }, body: VIDEO });
    const start = +m[1], end = m[2] ? +m[2] : VIDEO.length - 1;
    r.respond({ status: 206, contentType: 'video/mp4', headers: { 'Accept-Ranges': 'bytes', 'Content-Range': `bytes ${start}-${end}/${VIDEO.length}` }, body: VIDEO.subarray(start, end + 1) });
  });
  await page.evaluateOnNewDocument(mock);
  await page.goto('file:///' + UI + '/editor.html?id=v');
  await page.waitForFunction(() => document.getElementById('video').readyState >= 2, { timeout: 8000 });

  const box = await page.evaluate(() => {
    const v = document.getElementById('video').getBoundingClientRect(), s = document.getElementById('stage').getBoundingClientRect();
    return { vTop: v.top, vBottom: v.bottom, sTop: s.top, sBottom: s.bottom, vWidth: v.width, sWidth: s.width, win: innerWidth };
  });
  check('preview area spans the window', box.sWidth >= box.win - 2, JSON.stringify(box));
  check('video has a visible width', box.vWidth > box.sWidth / 2, JSON.stringify(box));
  check('video fits inside the preview area (controls visible)', box.vTop >= box.sTop && box.vBottom <= box.sBottom, JSON.stringify(box));

  await page.keyboard.press('Space');
  await new Promise(r => setTimeout(r, 1500));
  const t = await page.evaluate(() => ({ paused: document.getElementById('video').paused, t: document.getElementById('video').currentTime }));
  check('Space starts playback', !t.paused && t.t > 0.5, JSON.stringify(t));
  await page.keyboard.press('Space');
  check('Space pauses again', await page.evaluate(() => document.getElementById('video').paused));
  await page.screenshot({ path: path.join(__dirname, 'out', 'video_preview.png') });

  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  await browser.close();
  process.exit(failures ? 1 : 0);
})();
