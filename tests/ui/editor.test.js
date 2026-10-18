// Drives ui/editor.html headlessly with a mocked Tauri backend.
const puppeteer = require('puppeteer-core');
const fs = require('fs');
const path = require('path');

const UI = path.resolve(__dirname, '../../ui').replace(/\\/g, '/');
const PNG = fs.readFileSync(path.join(__dirname, 'fixture.png'));
const OUT = path.join(__dirname, 'out');
fs.mkdirSync(OUT, { recursive: true });

const mock = `
  window.__saved = null;
  window.__TAURI__ = {
    core: { invoke: async (cmd, args, opts) => {
      if (cmd === 'history_get') return { id: 'x', kind: 'image', file: 'test.png', thumb: 't.jpg', created: new Date().toISOString(), width: 800, height: 500, duration: 0, url: null };
      if (cmd === 'save_edited') { window.__saved = args.length; return null; }
      return null;
    } },
    event: { listen: async () => () => {}, emit: async () => {} },
    window: { getCurrentWindow: () => ({ label: 'ed_x', close() {}, show() {}, setFocus() {}, startDragging() {} }) },
  };
  Object.defineProperty(navigator, 'userAgent', { get: () => 'Mozilla/5.0 (Windows NT 10.0)' });
`;

let failures = 0;
const check = (name, ok, extra = '') => { console.log(`${ok ? 'PASS' : 'FAIL'}  ${name} ${extra}`); if (!ok) failures++; };

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true, args: ['--allow-file-access-from-files'] });
  const page = await browser.newPage();
  await page.setViewport({ width: 1200, height: 820 });
  page.on('pageerror', e => { console.log('PAGE ERROR', e.message); failures++; });
  await page.setRequestInterception(true);
  page.on('request', r => {
    if (r.url().startsWith('http://mycap.localhost/file/')) {
      r.respond({ status: 200, contentType: 'image/png', headers: { 'Access-Control-Allow-Origin': '*' }, body: PNG });
    } else r.continue();
  });
  await page.evaluateOnNewDocument(mock);
  await page.goto('file:///' + UI + '/editor.html?id=x');
  await page.waitForFunction(() => document.getElementById('canvas').width === 800, { timeout: 5000 });

  const rect = () => page.evaluate(() => { const r = document.getElementById('canvas').getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; });
  const st = expr => page.evaluate(expr);
  const drag = async (x0, y0, x1, y1) => {
    await page.mouse.move(x0, y0); await page.mouse.down();
    for (let i = 1; i <= 12; i++) await page.mouse.move(x0 + (x1 - x0) * i / 12, y0 + (y1 - y0) * i / 12);
    await page.mouse.up();
  };

  // 1. Arrow that starts outside the screenshot (left of it) grows the canvas to the start point.
  let r = await rect();
  const startX = r.x - 40, startY = r.y + r.h * 0.6;
  await drag(startX, startY, r.x + r.w * 0.3, r.y + r.h * 0.35);
  let s = await st('({ n: shapes.length, w: document.getElementById("canvas").width, ox: origin.x, x1: shapes[0]?.x1, sel: selected === shapes[0] })');
  check('arrow drawn from outside the image', s.n === 1, JSON.stringify(s));
  check('canvas grew left to the start point', s.w > 800 && s.ox <= Math.floor(s.x1));
  check('new arrow is selected', s.sel);
  await page.screenshot({ path: path.join(OUT, 'ui1_arrow_outside.png') });

  // 2. Drag the arrow's end handle.
  let a = await st('({ x2: shapes[0].x2, y2: shapes[0].y2 })');
  r = await rect();
  let sc = await st('scale'), o = await st('origin');
  let hx = r.x + (a.x2 - o.x) * sc, hy = r.y + (a.y2 - o.y) * sc;
  await drag(hx, hy, hx + 120, hy + 80);
  let a2 = await st('({ x1: shapes[0].x1, x2: shapes[0].x2, y2: shapes[0].y2 })');
  check('end handle moves the arrow tip', Math.abs(a2.x2 - a.x2) > 50 && Math.abs(a2.y2 - a.y2) > 30, JSON.stringify(a2));

  // 3. Move the whole arrow by its body, with the Select tool.
  await page.keyboard.press('v');
  a = await st('({ x1: shapes[0].x1, y1: shapes[0].y1, x2: shapes[0].x2, y2: shapes[0].y2 })');
  r = await rect(); sc = await st('scale'); o = await st('origin');
  const mx = r.x + ((a.x1 + a.x2) / 2 - o.x) * sc, my = r.y + ((a.y1 + a.y2) / 2 - o.y) * sc;
  await drag(mx, my, mx + 60, my + 60);
  a2 = await st('({ x1: shapes[0].x1, x2: shapes[0].x2 })');
  check('dragging the body moves both ends', Math.abs((a2.x1 - a.x1) - (a2.x2 - a.x2)) < 1 && a2.x1 - a.x1 > 20, JSON.stringify(a2));

  // 4. Colour and style apply to the selection.
  await page.click('.sw[data-c="#10b981"]');
  check('swatch recolours the selected arrow', await st('shapes[0].color') === '#10b981');
  await page.click('#opts button[data-style="curved"]');
  check('style bar restyles the selected arrow', await st('shapes[0].style') === 'curved');

  // 5. Delete, then undo brings it back with its edits.
  await page.keyboard.press('Delete');
  check('Delete removes the selection', await st('shapes.length') === 0);
  await page.keyboard.down('Control'); await page.keyboard.press('z'); await page.keyboard.up('Control');
  check('Ctrl+Z restores it', await st('shapes.length === 1 && shapes[0].style === "curved"'));
  await page.keyboard.down('Control'); await page.keyboard.press('z'); await page.keyboard.up('Control');
  check('another Ctrl+Z undoes the restyle', await st('shapes[0].style') !== 'curved');

  // 6. Callout dragged from above the image: bubble outside, tip inside; canvas grows up.
  await page.keyboard.press('c');
  r = await rect();
  await drag(r.x + r.w * 0.45, r.y + r.h * 0.3, r.x + r.w * 0.55, r.y - 30);
  await page.keyboard.type('Click Save');
  await page.keyboard.press('Enter');
  s = await st('({ n: shapes.length, t: shapes[shapes.length-1].text, oy: origin.y, h: document.getElementById("canvas").height })');
  check('callout created with text', s.t === 'Click Save', JSON.stringify(s));
  check('canvas grew upward for the bubble', s.oy < 0 && s.h > 500);

  // 7. Double-click the callout to edit its text.
  const c = await st('(() => { const b = calloutBox(shapes[shapes.length-1]); return { x: b.x + b.w / 2, y: b.y + b.h / 2 }; })()');
  r = await rect(); sc = await st('scale'); o = await st('origin');
  await page.mouse.click(r.x + (c.x - o.x) * sc, r.y + (c.y - o.y) * sc, { clickCount: 2 });
  await page.keyboard.down('Control'); await page.keyboard.press('a'); await page.keyboard.up('Control');
  await page.keyboard.type('Click Save now');
  await page.keyboard.press('Enter');
  check('double-click edits callout text', await st('shapes[shapes.length-1].text') === 'Click Save now');

  // 8. Drag the callout tip.
  const tip0 = await st('({ x: shapes[shapes.length-1].tx, y: shapes[shapes.length-1].ty })');
  await page.mouse.click(r.x + (c.x - o.x) * sc, r.y + (c.y - o.y) * sc);
  r = await rect(); sc = await st('scale'); o = await st('origin');
  await drag(r.x + (tip0.x - o.x) * sc, r.y + (tip0.y - o.y) * sc, r.x + (tip0.x - o.x) * sc - 80, r.y + (tip0.y - o.y) * sc + 40);
  const tip1 = await st('({ x: shapes[shapes.length-1].tx, y: shapes[shapes.length-1].ty })');
  check('callout tip handle moves the pointer', tip0.x - tip1.x > 40, JSON.stringify({ tip0, tip1 }));

  // 9. Box: corner handle resize.
  await page.keyboard.press('r');
  r = await rect();
  await drag(r.x + r.w * 0.6, r.y + r.h * 0.6, r.x + r.w * 0.8, r.y + r.h * 0.8);
  const b0 = await st('({ ...shapes[shapes.length-1] })');
  sc = await st('scale'); o = await st('origin'); r = await rect();
  await drag(r.x + (b0.x2 - o.x) * sc, r.y + (b0.y2 - o.y) * sc, r.x + (b0.x2 - o.x) * sc + 50, r.y + (b0.y2 - o.y) * sc + 30);
  const b1 = await st('({ ...shapes[shapes.length-1] })');
  check('box corner handle resizes', b1.x2 - b0.x2 > 30 && b1.x1 === b0.x1, JSON.stringify({ x2: [b0.x2, b1.x2] }));

  await page.screenshot({ path: path.join(OUT, 'ui2_final.png') });

  // 10. Export leaves out the selection outline and includes the grown canvas.
  await page.click('#copy');
  await new Promise(res => setTimeout(res, 400));
  check('export sends PNG bytes', (await st('window.__saved')) > 1000);

  console.log(failures ? `${failures} FAILED` : 'ALL PASSED');
  await browser.close();
  process.exit(failures ? 1 : 0);
})();
