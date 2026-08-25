// AUGECOIN Explorer — dependency-free canvas charts (line, bar, donut).
// No Chart.js. Handles DPR scaling, tooltips and theme-aware colors.

function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function setupCanvas(canvas) {
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  const ctx = canvas.getContext('2d');
  ctx.scale(dpr, dpr);
  return { ctx, w: rect.width, h: rect.height };
}

function themeColors() {
  return {
    grid: cssVar('--chart-grid') || 'rgba(148,163,184,0.15)',
    text: cssVar('--chart-text') || '#94a3b8',
    line: cssVar('--color-primary') || '#F97316',
    fill: cssVar('--chart-fill') || 'rgba(249,115,22,0.15)',
    bar: cssVar('--color-primary') || '#F97316',
  };
}

function niceTicks(max) {
  const step = Math.pow(10, Math.floor(Math.log10(max || 1)));
  const out = [];
  for (let i = 0; i <= 6; i++) out.push((step * i).toPrecision(2));
  return out;
}

/**
 * Draw a line/area chart.
 * @param {HTMLCanvasElement} canvas
 * @param {Array<{label:string, value:number}>} points
 */
export function lineChart(canvas, points, opts = {}) {
  const { ctx, w, h } = setupCanvas(canvas);
  const c = themeColors();
  const pad = { l: 44, r: 12, t: 12, b: 24 };
  const iw = w - pad.l - pad.r;
  const ih = h - pad.t - pad.b;

  ctx.clearRect(0, 0, w, h);
  if (!points || points.length === 0) return;

  const max = Math.max(...points.map((p) => p.value), 1);
  const yMax = max * 1.15;

  // grid + y labels
  ctx.font = '11px Inter, sans-serif';
  ctx.textAlign = 'right';
  ctx.textBaseline = 'middle';
  for (let i = 0; i <= 4; i++) {
    const y = pad.t + ih - (ih * i) / 4;
    ctx.strokeStyle = c.grid;
    ctx.beginPath();
    ctx.moveTo(pad.l, y);
    ctx.lineTo(w - pad.r, y);
    ctx.stroke();
    ctx.fillStyle = c.text;
    const val = (yMax * i) / 4;
    ctx.fillText(fmtTick(val), pad.l - 6, y);
  }

  const x = (i) => pad.l + (points.length === 1 ? iw / 2 : (iw * i) / (points.length - 1));
  const y = (v) => pad.t + ih - (ih * v) / yMax;

  // area fill
  ctx.beginPath();
  points.forEach((p, i) => (i === 0 ? ctx.moveTo(x(i), y(p.value)) : ctx.lineTo(x(i), y(p.value))));
  ctx.lineTo(x(points.length - 1), pad.t + ih);
  ctx.lineTo(x(0), pad.t + ih);
  ctx.closePath();
  ctx.fillStyle = c.fill;
  ctx.fill();

  // line
  ctx.beginPath();
  points.forEach((p, i) => (i === 0 ? ctx.moveTo(x(i), y(p.value)) : ctx.lineTo(x(i), y(p.value))));
  ctx.strokeStyle = c.line;
  ctx.lineWidth = 2;
  ctx.lineJoin = 'round';
  ctx.stroke();

  // dots
  points.forEach((p, i) => {
    ctx.beginPath();
    ctx.arc(x(i), y(p.value), 3, 0, Math.PI * 2);
    ctx.fillStyle = c.line;
    ctx.fill();
  });

  // x labels (sparse)
  const step = Math.max(1, Math.ceil(points.length / 8));
  ctx.textAlign = 'center';
  ctx.textBaseline = 'top';
  ctx.fillStyle = c.text;
  points.forEach((p, i) => {
    if (i % step === 0) ctx.fillText(p.label, x(i), pad.t + ih + 8);
  });
}

function fmtTick(v) {
  if (v >= 1e9) return (v / 1e9).toFixed(1) + 'B';
  if (v >= 1e6) return (v / 1e6).toFixed(1) + 'M';
  if (v >= 1e3) return (v / 1e3).toFixed(1) + 'K';
  return v.toFixed(0);
}

/**
 * Draw a vertical bar chart.
 * @param {HTMLCanvasElement} canvas
 * @param {Array<{label:string, value:number}>} bars
 */
export function barChart(canvas, bars, opts = {}) {
  const { ctx, w, h } = setupCanvas(canvas);
  const c = themeColors();
  const pad = { l: 40, r: 12, t: 12, b: 24 };
  const iw = w - pad.l - pad.r;
  const ih = h - pad.t - pad.b;

  ctx.clearRect(0, 0, w, h);
  if (!bars || bars.length === 0) return;

  const max = Math.max(...bars.map((b) => b.value), 1);
  const yMax = max * 1.15;
  const bw = iw / bars.length;
  const barW = Math.min(46, bw * 0.7);

  ctx.font = '11px Inter, sans-serif';
  ctx.textAlign = 'right';
  ctx.textBaseline = 'middle';
  for (let i = 0; i <= 4; i++) {
    const y = pad.t + ih - (ih * i) / 4;
    ctx.strokeStyle = c.grid;
    ctx.beginPath(); ctx.moveTo(pad.l, y); ctx.lineTo(w - pad.r, y); ctx.stroke();
    ctx.fillStyle = c.text;
    ctx.fillText(fmtTick((yMax * i) / 4), pad.l - 6, y);
  }

  bars.forEach((b, i) => {
    const x = pad.l + i * bw + (bw - barW) / 2;
    const bh = (ih * b.value) / yMax;
    const y = pad.t + ih - bh;
    ctx.fillStyle = b.color || c.bar;
    roundRect(ctx, x, y, barW, bh, 4);
    ctx.fill();
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';
    ctx.fillStyle = c.text;
    ctx.fillText(b.label, x + barW / 2, pad.t + ih + 8);
  });
}

/**
 * Draw a donut chart.
 * @param {HTMLCanvasElement} canvas
 * @param {Array<{label:string, value:number, color?:string}>} slices
 */
export function donutChart(canvas, slices) {
  const { ctx, w, h } = setupCanvas(canvas);
  ctx.clearRect(0, 0, w, h);
  if (!slices || slices.length === 0) return;
  const total = slices.reduce((a, s) => a + s.value, 0);
  if (total <= 0) return;
  const cx = w / 2, cy = h / 2;
  const r = Math.min(w, h) / 2 - 8;
  const inner = r * 0.62;
  let start = -Math.PI / 2;
  const palette = ['#F97316', '#22C55E', '#3B82F6', '#EF4444', '#A855F7', '#EAB308'];
  slices.forEach((s, i) => {
    const ang = (s.value / total) * Math.PI * 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.arc(cx, cy, r, start, start + ang);
    ctx.closePath();
    ctx.fillStyle = s.color || palette[i % palette.length];
    ctx.fill();
    start += ang;
  });
  // inner circle (donut hole)
  ctx.beginPath();
  ctx.arc(cx, cy, inner, 0, Math.PI * 2);
  ctx.fillStyle = cssVar('--bg-card') || '#1E293B';
  ctx.fill();
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

// ── Re-render all charts on theme change ───────────────────────────────

export function registerChartsForTheme(fns) {
  const observer = new MutationObserver(() => fns.forEach((f) => { try { f(); } catch {} }));
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  return observer;
}
