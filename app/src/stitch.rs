//! Scrolling capture: join successive frames of a scrolled region into one tall image.
//!
//! Each frame is reduced to a per-row signature (mean brightness in a few columns bands, scrollbar
//! excluded). Between two frames we look for the vertical shift that lines the rows up, then append
//! only the rows that scrolled into view. Rows that stay put at the top and bottom (sticky headers,
//! footers, toolbars) are detected on the first move and kept once.
use image::RgbaImage;

const BINS: usize = 32;
/// Rows that must overlap for a shift to count.
const MIN_OVERLAP: usize = 24;
/// Mean brightness difference per band still treated as "the same row" (allows for re-rasterised text).
const ROW_TOLERANCE: f32 = 2.5;
/// Mean row difference above which no shift is accepted.
const SHIFT_LIMIT: f32 = 7.0;
/// Overlaps with fewer rows that differ from their neighbour are blank space and could match anywhere.
const MIN_DETAIL_ROWS: usize = 6;

type Sig = Vec<[u8; BINS]>;

fn signature(img: &RgbaImage) -> Sig {
    let (w, h) = (img.width() as usize, img.height() as usize);
    // Leave out the right edge, where the scrollbar thumb moves.
    let usable = (w - (w / 25).clamp(8, 24).min(w / 2)).max(1);
    let raw = img.as_raw();
    (0..h)
        .map(|y| {
            let mut sums = [0u32; BINS];
            let mut counts = [0u32; BINS];
            let row = &raw[y * w * 4..(y * w + usable) * 4];
            for (x, px) in row.as_chunks::<4>().0.iter().enumerate() {
                let b = x * BINS / usable;
                sums[b] += (px[0] as u32 * 3 + px[1] as u32 * 6 + px[2] as u32) / 10;
                counts[b] += 1;
            }
            let mut out = [0u8; BINS];
            for b in 0..BINS {
                out[b] = sums[b].checked_div(counts[b]).unwrap_or(0) as u8;
            }
            out
        })
        .collect()
}

fn row_diff(a: &[u8; BINS], b: &[u8; BINS]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs()).sum::<u32>() as f32 / BINS as f32
}

/// Rows equal at the same position in both frames, counted from the top and from the bottom.
fn fixed_edges(a: &Sig, b: &Sig) -> (usize, usize) {
    let h = a.len();
    let top = (0..h).take_while(|&i| row_diff(&a[i], &b[i]) < ROW_TOLERANCE).count();
    let bottom = (0..h - top).take_while(|&i| row_diff(&a[h - 1 - i], &b[h - 1 - i]) < ROW_TOLERANCE).count();
    (top, bottom)
}

/// How far the content in `b` moved up relative to `a`, within rows `top..bottom`: the shift whose
/// overlapping rows differ least. At fractional display scaling text is redrawn at sub-pixel
/// offsets, so rows never match exactly; the best shift just has to be clearly close.
fn find_shift(a: &Sig, b: &Sig, top: usize, bottom: usize) -> Option<usize> {
    let body = bottom.saturating_sub(top);
    if body <= MIN_OVERLAP {
        return None;
    }
    let mut best: Option<(usize, f32)> = None;
    for d in 1..=body - MIN_OVERLAP {
        let rows = body - d;
        // Stop adding up once this shift can no longer beat the best one (or the limit).
        let bound = best.map_or(SHIFT_LIMIT, |(_, m)| m.min(SHIFT_LIMIT)) * rows as f32;
        let mut total = 0.0;
        let mut detail = 0;
        let mut pruned = false;
        for i in top..bottom - d {
            total += row_diff(&b[i], &a[i + d]);
            if total > bound {
                pruned = true;
                break;
            }
            if i > top && row_diff(&b[i], &b[i - 1]) > ROW_TOLERANCE * 2.0 {
                detail += 1;
            }
        }
        if !pruned && detail >= MIN_DETAIL_ROWS {
            let mean = total / rows as f32;
            if best.is_none_or(|(_, m)| mean < m) {
                best = Some((d, mean));
            }
        }
    }
    best.map(|(d, _)| d)
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// The image grew by this many rows.
    Grew(usize),
    /// Nothing moved (or it moved back up).
    Same,
    /// The frames could not be lined up: scrolled too far at once, or the content changed.
    Lost,
}

pub struct Stitcher {
    out: RgbaImage,
    last: RgbaImage,
    last_sig: Sig,
    /// Rows at the top / bottom that stay put while scrolling (sticky bars). The header is taken
    /// from the first move; the footer only ever shrinks, since blank margins can look fixed at first.
    /// The bottom rows of the last frame are not in `out` yet; `finish` adds them.
    edges: Option<(usize, usize)>,
    pub max_height: u32,
}

impl Stitcher {
    pub fn new(first: RgbaImage) -> Self {
        let last_sig = signature(&first);
        Self { out: first.clone(), last: first, last_sig, edges: None, max_height: 40_000 }
    }

    pub fn height(&self) -> u32 {
        self.out.height() + self.edges.map(|(_, f)| f as u32).unwrap_or(0)
    }

    pub fn full(&self) -> bool {
        self.height() >= self.max_height
    }

    pub fn push(&mut self, frame: RgbaImage) -> Step {
        if frame.dimensions() != self.last.dimensions() {
            return Step::Lost;
        }
        let sig = signature(&frame);
        let h = sig.len();
        let (t, b) = fixed_edges(&self.last_sig, &sig);
        if t >= h {
            return Step::Same;
        }
        // Big "fixed" bands are more likely blank page than a toolbar.
        let (mut top, mut bottom) = (t.min(h / 3), b.min(h / 3));
        if let Some((et, eb)) = self.edges {
            // The header is settled by the first move: it only narrows what is compared, and a sticky
            // bar redrawn at a sub-pixel offset can look shorter on later frames. The footer decides
            // which rows are kept, so it may only shrink.
            top = et;
            bottom = bottom.min(eb);
        }
        // Unmoved apart from small changes (a blinking caret, a hover effect).
        let still = (top..h - bottom).map(|i| row_diff(&self.last_sig[i], &sig[i])).sum::<f32>() / (h - bottom - top).max(1) as f32;
        if still < ROW_TOLERANCE / 2.0 {
            return Step::Same;
        }
        // A guessed header that is really blank page can leave too little overlap after a big
        // jump, so try again comparing from the top.
        let Some(d) = find_shift(&self.last_sig, &sig, top, h - bottom).or_else(|| (top > 0).then(|| find_shift(&self.last_sig, &sig, 0, h - bottom)).flatten()) else {
            // Keep the older frame as the reference so a slower scroll can still line up.
            return Step::Lost;
        };
        // Rows of the last frame not yet in `out`; the first frame was stored whole.
        let pending = match self.edges {
            Some((_, eb)) => eb,
            None => {
                self.out = image::imageops::crop_imm(&self.out, 0, 0, self.out.width(), (h - bottom) as u32).to_image();
                bottom
            }
        };
        self.edges = Some((top, bottom));
        // `out` ends at row h - pending - d of this frame; take everything down to the sticky footer.
        let from = h - pending - d;
        let room = self.max_height.saturating_sub(self.out.height() + bottom as u32) as usize;
        let add = (h - bottom - from).min(room);
        if add > 0 {
            let new_rows = image::imageops::crop_imm(&frame, 0, from as u32, frame.width(), add as u32).to_image();
            let mut grown = RgbaImage::new(self.out.width(), self.out.height() + add as u32);
            image::imageops::replace(&mut grown, &self.out, 0, 0);
            image::imageops::replace(&mut grown, &new_rows, 0, self.out.height() as i64);
            self.out = grown;
        }
        self.last = frame;
        self.last_sig = sig;
        if add > 0 { Step::Grew(add) } else { Step::Same }
    }

    /// The stitched image, with the sticky footer from the last frame.
    pub fn finish(self) -> RgbaImage {
        let Some((_, bottom)) = self.edges.filter(|(_, b)| *b > 0) else { return self.out };
        let h = self.last.height();
        let footer = image::imageops::crop_imm(&self.last, 0, h - bottom as u32, self.last.width(), bottom as u32).to_image();
        let mut img = RgbaImage::new(self.out.width(), self.out.height() + bottom as u32);
        image::imageops::replace(&mut img, &self.out, 0, 0);
        image::imageops::replace(&mut img, &footer, 0, self.out.height() as i64);
        img
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A long "page": text-like stripes of varying shade with blank gaps, deterministic.
    fn page(w: u32, h: u32) -> RgbaImage {
        let mut seed = 12345u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            seed >> 16
        };
        let mut img = RgbaImage::from_pixel(w, h, Rgba([250, 250, 250, 255]));
        let mut y = 10;
        while y < h - 20 {
            let lh = 8 + rnd() % 10;
            let len = w / 4 + rnd() % (w / 2);
            let x0 = 10 + rnd() % 30;
            let shade = (rnd() % 160) as u8;
            for yy in y..(y + lh).min(h) {
                for x in x0..(x0 + len).min(w) {
                    if (x / 3 + yy) % 4 != 0 {
                        img.put_pixel(x, yy, Rgba([shade, shade / 2, 255 - shade, 255]));
                    }
                }
            }
            y += lh + 6 + rnd() % 30;
        }
        img
    }

    fn view(p: &RgbaImage, y: u32, h: u32) -> RgbaImage {
        image::imageops::crop_imm(p, 0, y, p.width(), h).to_image()
    }

    fn with_bars(mut f: RgbaImage, head: u32, foot: u32) -> RgbaImage {
        let (w, h) = f.dimensions();
        for y in (0..head).chain(h - foot..h) {
            for x in 0..w {
                f.put_pixel(x, y, Rgba([30, 40, 60 + (x % 50) as u8, 255]));
            }
        }
        f
    }

    #[test]
    fn stitches_plain_scroll() {
        let p = page(400, 2000);
        let mut s = Stitcher::new(view(&p, 0, 300));
        let mut y = 0;
        for step in [40, 90, 3, 150, 0, 210, 120, 77] {
            y += step;
            let r = s.push(view(&p, y, 300));
            if step == 0 { assert_eq!(r, Step::Same) } else { assert!(matches!(r, Step::Grew(_)), "step {step}: {r:?}") }
        }
        let out = s.finish();
        assert_eq!(out.height(), y + 300);
        assert!(out == view(&p, 0, y + 300), "stitched image differs from the page");
    }

    #[test]
    fn sticky_header_and_footer_kept_once() {
        let p = page(400, 1500);
        let (head, foot) = (40, 30);
        let mut s = Stitcher::new(with_bars(view(&p, 0, 300), head, foot));
        let mut y = 0;
        for step in [60, 60, 100, 45] {
            y += step;
            assert!(matches!(s.push(with_bars(view(&p, y, 300), head, foot)), Step::Grew(_)));
        }
        let out = s.finish();
        assert_eq!(out.height(), 300 + y);
        // The body between the bars is the page from row `head` on.
        let body = view(&out, head, out.height() - head - foot);
        assert!(body == view(&p, head, 300 + y - head - foot));
    }

    #[test]
    fn too_far_is_lost_then_recovers() {
        let p = page(400, 2000);
        let mut s = Stitcher::new(view(&p, 0, 300));
        assert_eq!(s.push(view(&p, 600, 300)), Step::Lost);
        assert_eq!(s.push(view(&p, 100, 300)), Step::Grew(100));
    }

    #[test]
    fn scrolling_back_up_adds_nothing() {
        let p = page(400, 2000);
        let mut s = Stitcher::new(view(&p, 200, 300));
        assert!(matches!(s.push(view(&p, 100, 300)), Step::Same | Step::Lost));
        assert_eq!(s.height(), 300);
    }

    #[test]
    fn stops_at_max_height() {
        let p = page(200, 3000);
        let mut s = Stitcher::new(view(&p, 0, 300));
        s.max_height = 500;
        assert_eq!(s.push(view(&p, 150, 300)), Step::Grew(150));
        assert_eq!(s.push(view(&p, 300, 300)), Step::Grew(50));
        assert!(s.full());
        assert_eq!(s.finish().height(), 500);
    }

    /// Real browser frames from tests/ui/scrollframes.js (headless Chrome, sticky header, real text).
    #[test]
    fn real_browser_frames() {
        let Ok(dir) = std::env::var("MYCAPTURE_SCROLL_FRAMES") else { return };
        let dir = std::path::PathBuf::from(dir);
        let load = |name: &str| image::open(dir.join(name)).unwrap().to_rgba8();
        let mut frames: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.starts_with("frame")).collect();
        frames.sort();
        let mut s = Stitcher::new(load(&frames[0]));
        let mut lost = 0;
        for f in &frames[1..] {
            if s.push(load(f)) == Step::Lost {
                lost += 1;
            }
        }
        let out = s.finish();
        let truth = load("full.png");
        out.save(dir.join("stitched.png")).unwrap();
        println!("{} frames, {lost} lost, stitched {}x{}, page {}x{}", frames.len(), out.width(), out.height(), truth.width(), truth.height());
        assert_eq!(lost, 0);
        assert_eq!(out.height(), truth.height());
        // Compare a few bands of rows against the full-page render.
        for y in (0..truth.height()).step_by(97) {
            let a = image::imageops::crop_imm(&out, 0, y, out.width() - 30, 1).to_image();
            let b = image::imageops::crop_imm(&truth, 0, y, out.width() - 30, 1).to_image();
            let diff: u64 = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum();
            assert!(diff / (a.as_raw().len() as u64) < 4, "row {y} differs from the page");
        }
    }
}
