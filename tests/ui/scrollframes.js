// Renders a long page with a sticky header in headless Chrome and saves the viewport at uneven
// scroll steps (frame###.png) plus the whole page (full.png), for the stitcher's real-frames test:
//   node scrollframes.js <outdir> && MYCAPTURE_SCROLL_FRAMES=<outdir> cargo test --lib real_browser_frames
const puppeteer = require('puppeteer-core');
const fs = require('fs');
const path = require('path');

const out = process.argv[2] || path.join(__dirname, 'out', 'scroll');
const scale = Number(process.env.SCALE || 1);
fs.mkdirSync(out, { recursive: true });
const rows = Array.from({ length: 90 }, (_, i) => `<p style="margin:0;padding:7px 14px;background:hsl(${i * 37 % 360},55%,${88 + i % 3 * 3}%)">Row ${i + 1}: the quick brown fox jumps over the lazy dog — ${'lorem ipsum '.repeat(1 + i % 4)}</p>`).join('');
const html = `<!doctype html><body style="margin:0;font:15px 'Segoe UI',sans-serif">
  <div style="position:sticky;top:0;background:#12233d;color:#fff;padding:14px 16px;font-weight:600">Sticky header · myCapture test</div>
  ${rows}<footer style="padding:30px;background:#1f2937;color:#cbd5e1">End of page</footer></body>`;

(async () => {
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME_PATH || 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 640, height: 420, deviceScaleFactor: scale });
  await page.setContent(html);
  const total = await page.evaluate(() => document.documentElement.scrollHeight);
  let y = 0, i = 0;
  const steps = [37, 120, 64, 200, 15, 180, 90, 150, 33, 240, 110];
  for (;;) {
    await page.evaluate(v => window.scrollTo(0, v), y);
    await page.screenshot({ path: path.join(out, `frame${String(i++).padStart(3, '0')}.png`) });
    if (y >= total - 420) break;
    y = Math.min(total - 420, y + steps[i % steps.length]);
  }
  // Ground truth: the same page without the sticky header repeating.
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.setViewport({ width: 640, height: total, deviceScaleFactor: scale });
  await page.screenshot({ path: path.join(out, 'full.png') });
  console.log(`${i} frames, page ${total}px -> ${out}`);
  await browser.close();
})();
