// The bodies the launcher can show. The moon is drawn from its PNG layers in moon.html;
// every other body is painted here from a procedural map onto a lit, turning sphere.

const BODIES = [
  { id: 'moon', name: 'Moon', es: 'Luna', halo: '#c8e4ff' },
  { id: 'sun', name: 'Sun', es: 'Sol', halo: '#ffb347', glow: 'rgba(255, 170, 60, .95)', emissive: true },
  { id: 'mercury', name: 'Mercury', es: 'Mercurio', halo: '#d6cfc4', glow: 'rgba(200, 192, 180, .55)' },
  { id: 'venus', name: 'Venus', es: 'Venus', halo: '#ffe2a8', glow: 'rgba(255, 222, 160, .6)', atm: [255, 236, 190] },
  { id: 'earth', name: 'Earth', es: 'Tierra', halo: '#8fd0ff', glow: 'rgba(110, 180, 255, .65)', atm: [140, 200, 255], spec: true },
  { id: 'mars', name: 'Mars', es: 'Marte', halo: '#ff9b6b', glow: 'rgba(240, 120, 70, .55)', atm: [255, 170, 130] },
  { id: 'jupiter', name: 'Jupiter', es: 'Júpiter', halo: '#ffd9a8', glow: 'rgba(240, 200, 150, .55)' },
  { id: 'saturn', name: 'Saturn', es: 'Saturno', halo: '#ffe6b0', glow: 'rgba(240, 214, 150, .3)', r: .44,
    ring: { inner: 1.24, outer: 2.1, tilt: .36, angle: -18 } },
  { id: 'uranus', name: 'Uranus', es: 'Urano', halo: '#a8f0f5', glow: 'rgba(140, 225, 235, .6)', atm: [200, 250, 255], r: .58,
    ring: { inner: 1.48, outer: 1.6, tilt: .3, angle: 74, faint: true } },
  { id: 'neptune', name: 'Neptune', es: 'Neptuno', halo: '#7fa8ff', glow: 'rgba(80, 130, 255, .65)', atm: [150, 190, 255], spec: true },
  { id: 'pluto', name: 'Pluto', es: 'Plutón', halo: '#f1dcc2', glow: 'rgba(225, 200, 170, .5)' },
];
const bodyOf = id => BODIES.find(b => b.id === id) || BODIES[0];

// ---------- procedural maps (equirectangular, wraps around in longitude) ----------
const MAP_W = 256, MAP_H = 128;

function rng(seed) {
  return () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 4294967296; };
}
// Value noise that tiles across the map's width, summed over octaves.
function noiseField(seed, cells) {
  const r = rng(seed), lat = Array.from({ length: (cells + 1) * (cells + 1) }, r);
  const at = (i, j) => lat[(j % (cells + 1)) * (cells + 1) + (i % cells)];
  const s = t => t * t * (3 - 2 * t);
  return (u, v) => {
    const x = u * cells, y = v * cells, i = Math.floor(x), j = Math.floor(y), fx = s(x - i), fy = s(y - j);
    const a = at(i, j), b = at(i + 1, j), c = at(i, j + 1), d = at(i + 1, j + 1);
    return a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy;
  };
}
function fbm(seed, base = 4, oct = 5) {
  const layers = Array.from({ length: oct }, (_, k) => noiseField(seed + k * 101, base << k));
  return (u, v) => {
    let sum = 0, amp = .5, norm = 0;
    for (const n of layers) { sum += n(u, v) * amp; norm += amp; amp *= .5; }
    return sum / norm;
  };
}
const mix = (a, b, t) => a.map((x, i) => x + (b[i] - x) * Math.min(1, Math.max(0, t)));
const smooth = (e0, e1, x) => { const t = Math.min(1, Math.max(0, (x - e0) / (e1 - e0))); return t * t * (3 - 2 * t); };
// Distance in the map from (u, v) to a feature, wrapping in longitude; lon is scaled by cos(lat).
function featureDist(u, v, fu, fv) {
  let du = Math.abs(u - fu); du = Math.min(du, 1 - du);
  return Math.hypot(du * 2 * Math.cos((v - .5) * Math.PI), v - fv);
}
function craters(seed, n, min, max) {
  const r = rng(seed);
  return Array.from({ length: n }, () => ({ u: r(), v: .15 + r() * .7, s: min + r() * (max - min) }));
}
function craterShade(list, u, v) {
  let k = 0;
  for (const c of list) {
    const d = featureDist(u, v, c.u, c.v) / c.s;
    if (d < 1) k += d > .75 ? .22 : -.18 * (1 - d * d);
  }
  return k;
}

const PAINT = {
  sun() {
    const g = fbm(11, 8, 4), spots = craters(5, 6, .018, .035).map(s => ({ ...s, v: .5 + (s.v - .5) * .45 }));
    return (u, v) => {
      let c = mix([230, 92, 10], [255, 214, 92], g(u, v) * 1.6 - .25);
      for (const s of spots) {
        const d = featureDist(u, v, s.u, s.v) / s.s;
        if (d < 1.6) c = mix(c, d < .7 ? [120, 40, 6] : [200, 90, 20], d < .7 ? .85 : (1.6 - d) * .6);
      }
      return c;
    };
  },
  mercury() {
    const g = fbm(21, 4, 5), cr = craters(23, 55, .015, .06);
    return (u, v) => mix([92, 86, 80], [190, 180, 168], g(u, v) * 1.3 - .1 + craterShade(cr, u, v));
  },
  venus() {
    const g = fbm(31, 3, 5);
    return (u, v) => {
      const w = g(u * .9 + .07 * Math.sin(v * 9), v * 1.4);
      return mix([206, 160, 88], [252, 232, 180], w * 1.5 - .15 + .12 * Math.sin(v * 22 + w * 6));
    };
  },
  earth() {
    const land = fbm(41, 4, 6), cloud = fbm(47, 5, 5), dry = fbm(43, 6, 3);
    return (u, v) => {
      const lat = Math.abs(v - .5) * 2, h = land(u, v);
      let c;
      if (h < .5) c = mix([12, 44, 112], [30, 102, 178], (h - .3) * 4.5);
      else c = mix([52, 118, 46], [168, 140, 84], dry(u, v) * 1.4 - .35 + (lat < .45 ? .25 - lat : 0) * 1.2);
      if (lat > .8) c = mix(c, [236, 242, 250], smooth(.8, .88, lat + (h - .5) * .2));
      const cl = cloud(u * 1.2, v);
      return mix(c, [250, 252, 255], smooth(.6, .78, cl) * .8);
    };
  },
  mars() {
    const g = fbm(51, 4, 6), cr = craters(57, 18, .02, .05);
    return (u, v) => {
      const lat = Math.abs(v - .5) * 2, n = g(u, v);
      let c = mix([118, 46, 26], [214, 112, 62], n * 1.6 - .2 + craterShade(cr, u, v) * .6);
      c = mix(c, [86, 40, 30], smooth(.55, .7, g(u + .3, v * .7)) * .55);
      return mix(c, [246, 240, 236], smooth(.84, .9, lat + n * .08));
    };
  },
  jupiter() {
    const g = fbm(61, 4, 5);
    const pal = [[238, 226, 204], [196, 152, 110], [244, 236, 220], [168, 112, 74], [226, 200, 160], [140, 96, 70]];
    return (u, v) => {
      const t = v * 9 + (g(u, v * 2) - .5) * 1.1 + .25 * Math.sin(u * 2 * Math.PI * 3 + v * 30) * g(u, v);
      const i = Math.floor(t), f = smooth(.15, .85, t - i);
      let c = mix(pal[((i % 6) + 6) % 6], pal[((i + 1) % 6 + 6) % 6], f);
      const d = Math.hypot(Math.min(Math.abs(u - .62), 1 - Math.abs(u - .62)) * 2 / 1.9, (v - .66) * 2 / 1.0);
      if (d < .09) c = mix(c, d < .055 ? [196, 86, 56] : [236, 196, 170], d < .055 ? .9 : (.09 - d) * 20);
      return c;
    };
  },
  saturn() {
    const g = fbm(71, 3, 4);
    return (u, v) => mix([200, 160, 96], [246, 224, 170], .55 + .35 * Math.sin(v * 46 + (g(u, v) - .5) * 2) + (g(u, v) - .5) * .4);
  },
  uranus() {
    const g = fbm(81, 3, 3);
    return (u, v) => mix([112, 196, 206], [186, 236, 238], .55 + .18 * Math.sin(v * 30) + (g(u, v) - .5) * .4 + (Math.abs(v - .5) < .16 ? -.1 : .1));
  },
  neptune() {
    const g = fbm(91, 4, 5);
    return (u, v) => {
      let c = mix([28, 56, 168], [86, 136, 232], .5 + .25 * Math.sin(v * 26 + g(u, v) * 3) + (g(u, v) - .5) * .5);
      const d = Math.hypot(Math.min(Math.abs(u - .3), 1 - Math.abs(u - .3)) * 2 / 1.6, (v - .64) * 2);
      if (d < .07) c = mix(c, [14, 26, 96], (.07 - d) * 25);
      return mix(c, [236, 244, 255], smooth(.68, .78, g(u * 2, v * .5)) * smooth(.3, .1, Math.abs(Math.sin(v * 14))) * .8);
    };
  },
  pluto() {
    const g = fbm(101, 4, 5);
    return (u, v) => {
      let c = mix([132, 92, 66], [222, 196, 168], g(u, v) * 1.5 - .2);
      const du = Math.min(Math.abs(u - .4), 1 - Math.abs(u - .4)) * 2, dv = v - .56;
      const heart = Math.hypot(du * 1.25, dv * 1.6 + du * .6) < .16 || Math.hypot(du * 1.25 - .1, dv * 1.6) < .14;
      if (heart) c = mix(c, [246, 236, 222], .8);
      if (Math.abs(v - .54) < .08 && !heart) c = mix(c, [86, 46, 32], smooth(.5, .62, g(u * 1.5, v)) * .8);
      return c;
    };
  },
};

const maps = {};
function mapOf(id) {
  if (maps[id]) return maps[id];
  const paint = PAINT[id](), data = new Float32Array(MAP_W * MAP_H * 3);
  for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
    const c = paint(x / MAP_W, (y + .5) / MAP_H), o = (y * MAP_W + x) * 3;
    data[o] = c[0]; data[o + 1] = c[1]; data[o + 2] = c[2];
  }
  return (maps[id] = data);
}

// ---------- sphere ----------
// Precomputes, for every canvas pixel, where it lands on the map and how much light it gets.
// The body then turns by sliding the map's longitude, so each frame is a cheap lookup.
class Globe {
  constructor(canvas, id) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.set(id);
  }

  set(id) {
    const b = this.body = bodyOf(id), n = this.canvas.width, map = this.map = mapOf(b.id);
    this.img = this.ctx.createImageData(n, n);
    const R = (b.r || .69) * n / 2, c = n / 2, tilt = .32, ct = Math.cos(tilt), st = Math.sin(tilt);
    const L = [-.3, -.38, .875], px = [];
    const ring = b.ring, ra = ring && ring.angle * Math.PI / 180;
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
      const nx = (x + .5 - c) / R, ny = (y + .5 - c) / R, d2 = nx * nx + ny * ny;
      const p = { i: (y * n + x) * 4 };
      if (d2 < 1.08) {
        const nz = Math.sqrt(Math.max(0, 1 - d2));
        // Lean the pole towards the viewer so the turning reads as a sphere.
        const yy = ny * ct - nz * st, zz = ny * st + nz * ct;
        p.u = (Math.atan2(nx, zz) / (2 * Math.PI) + .5) * MAP_W;
        p.v = Math.min(MAP_H - 1, Math.max(0, (Math.asin(Math.max(-1, Math.min(1, yy))) / Math.PI + .5) * MAP_H - .5));
        p.a = Math.min(1, Math.max(0, (1 - Math.sqrt(d2)) * R + .5));
        if (b.emissive) p.k = .62 + .48 * Math.pow(nz, .6);
        else {
          const dot = lit(nx, ny, nz, L);
          p.k = .1 + 1.08 * Math.pow(dot, .85);
          if (b.spec) p.s = Math.pow(Math.max(0, 2 * dot * nz - L[2]), 24) * 120;
        }
        if (b.atm) p.m = Math.pow(1 - nz, 2.4) * .75 * (.35 + lit(nx, ny, nz, L));
      }
      if (ring) {
        // Rings: an ellipse in the picture, lit flat; the half nearer the viewer covers the planet.
        const rx = nx * Math.cos(ra) + ny * Math.sin(ra), ry = -nx * Math.sin(ra) + ny * Math.cos(ra);
        const rr = Math.hypot(rx, ry / ring.tilt);
        if (rr > ring.inner && rr < ring.outer) {
          const t = (rr - ring.inner) / (ring.outer - ring.inner);
          let alpha = ring.faint ? .45 : (t > .62 && t < .68 ? .12 : .9 - .35 * Math.abs(Math.sin(t * 19)));
          alpha *= Math.min(1, (rr - ring.inner) * R, (ring.outer - rr) * R);
          const col = ring.faint ? [190, 236, 240] : mix([170, 140, 96], [240, 224, 186], .6 + .4 * Math.sin(t * 11));
          p.ring = { col, alpha, front: ry > 0 };
        }
      }
      if (p.u !== undefined || p.ring) px.push(p);
    }
    this.px = px;
    this.offset = null;
  }

  // Draw with the map turned by `turn` (0..1 of a full revolution).
  draw(turn) {
    const off = ((turn % 1) + 1) % 1 * MAP_W;
    if (this.offset !== null && Math.abs(off - this.offset) < .15) return;
    this.offset = off;
    const d = this.img.data, map = this.map;
    d.fill(0);
    for (const p of this.px) {
      let r = 0, g = 0, b = 0, a = 0;
      if (p.u !== undefined) {
        const u = p.u + off, x0 = Math.floor(u), fx = u - x0, y0 = Math.floor(p.v), fy = p.v - y0;
        const i00 = (y0 * MAP_W + (x0 % MAP_W)) * 3, i10 = (y0 * MAP_W + ((x0 + 1) % MAP_W)) * 3;
        const y1 = Math.min(MAP_H - 1, y0 + 1), i01 = (y1 * MAP_W + (x0 % MAP_W)) * 3, i11 = (y1 * MAP_W + ((x0 + 1) % MAP_W)) * 3;
        const s = (o) => (map[i00 + o] * (1 - fx) + map[i10 + o] * fx) * (1 - fy) + (map[i01 + o] * (1 - fx) + map[i11 + o] * fx) * fy;
        r = s(0) * p.k; g = s(1) * p.k; b = s(2) * p.k;
        if (p.s) { r += p.s; g += p.s; b += p.s; }
        if (p.m) { const t = p.m, c = this.body.atm; r += (c[0] - r) * t; g += (c[1] - g) * t; b += (c[2] - b) * t; }
        a = p.a;
      }
      if (p.ring) {
        // Composite the ring over the planet (front half) or under it (back half).
        const { col, alpha, front } = p.ring;
        const [ta, tc, ba, bc] = front ? [alpha, col, a, [r, g, b]] : [a, [r, g, b], alpha, col];
        const out = ta + ba * (1 - ta), w = out ? ba * (1 - ta) / out : 0, v = out ? ta / out : 0;
        r = tc[0] * v + bc[0] * w; g = tc[1] * v + bc[1] * w; b = tc[2] * v + bc[2] * w;
        a = out;
      }
      d[p.i] = r; d[p.i + 1] = g; d[p.i + 2] = b; d[p.i + 3] = a * 255;
    }
    this.ctx.putImageData(this.img, 0, 0);
  }
}
function lit(nx, ny, nz, L) { return Math.max(0, nx * L[0] + ny * L[1] + nz * L[2]); }

// A still picture of a body, e.g. for the picker in Settings.
function paintBody(canvas, id, turn = .15) {
  if (id === 'moon') {
    const ctx = canvas.getContext('2d'), img = new Image();
    img.onload = () => { ctx.clearRect(0, 0, canvas.width, canvas.height); ctx.drawImage(img, 0, 0, canvas.width, canvas.height); };
    img.src = 'moon.png';
    return;
  }
  new Globe(canvas, id).draw(turn);
}
