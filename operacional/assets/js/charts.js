// AUGECOIN Operacional — lightweight SVG charts (no external dependencies).
//
// Each helper returns an SVG string. Charts are responsive (viewBox + 100%
// width) and theme-aware via CSS variables.

const W = 600;
const H = 220;
const PAD = { top: 14, right: 12, bottom: 24, left: 46 };

function fmtCompact(n) {
  const abs = Math.abs(n);
  if (abs >= 1e9) return (n / 1e9).toFixed(1) + 'B';
  if (abs >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (abs >= 1e3) return (n / 1e3).toFixed(1) + 'K';
  return String(Math.round(n));
}

function scaleDomain(values) {
  const max = Math.max(0, ...values);
  if (max === 0) return [0, 1];
  return [0, max];
}

function gridLines(domain) {
  const [lo, hi] = domain;
  const n = 4;
  let out = '';
  for (let i = 0; i <= n; i++) {
    const v = lo + ((hi - lo) * i) / n;
    const y = PAD.top + (H - PAD.top - PAD.bottom) * (1 - i / n);
    out += `<line x1="${PAD.left}" y1="${y.toFixed(1)}" x2="${W - PAD.right}" y2="${y.toFixed(1)}" stroke="var(--chart-grid)"/>`;
    out += `<text x="${PAD.left - 8}" y="${(y + 4).toFixed(1)}" text-anchor="end" font-size="10" fill="var(--chart-text)">${fmtCompact(v)}</text>`;
  }
  return out;
}

/**
 * Line chart for a single series.
 * @param {number[]} data
 * @param {string[]} [labels]
 * @param {string} [color]
 * @param {boolean} [area]
 */
export function lineChart({ data, labels = [], color = 'var(--color-primary)', area = true }) {
  if (!data || data.length === 0) return '';
  const domain = scaleDomain(data);
  const innerW = W - PAD.left - PAD.right;
  const innerH = H - PAD.top - PAD.bottom;
  const step = data.length > 1 ? innerW / (data.length - 1) : innerW;
  const [lo, hi] = domain;
  const span = hi - lo;

  const pts = data.map((v, i) => {
    const x = PAD.left + i * step;
    const y = PAD.top + innerH * (1 - (v - lo) / span);
    return [x, y];
  });

  const line = pts.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(' ');
  const areaPath = area
    ? `<polygon points="${PAD.left},${H - PAD.bottom} ${line} ${W - PAD.right},${H - PAD.bottom}" fill="var(--chart-fill)" stroke="none"/>`
    : '';

  const xLabels = labels.length
    ? labels.map((l, i) => {
        const x = PAD.left + i * step;
        return `<text x="${x.toFixed(1)}" y="${H - 6}" text-anchor="middle" font-size="10" fill="var(--chart-text)">${l}</text>`;
      }).join('')
    : '';

  return `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" style="width:100%;height:100%;display:block">
    ${gridLines(domain)}
    ${areaPath}
    <polyline points="${line}" fill="none" stroke="${color}" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"/>
    ${pts.map(([x, y]) => `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="3" fill="${color}"/>`).join('')}
    ${xLabels}
  </svg>`;
}

/**
 * Bar chart for a single series.
 * @param {number[]} data
 * @param {string[]} [labels]
 * @param {string} [color]
 */
export function barChart({ data, labels = [], color = 'var(--color-primary)' }) {
  if (!data || data.length === 0) return '';
  const domain = scaleDomain(data);
  const innerW = W - PAD.left - PAD.right;
  const innerH = H - PAD.top - PAD.bottom;
  const [lo, hi] = domain;
  const span = hi - lo;
  const slot = innerW / data.length;
  const barW = Math.max(4, slot * 0.6);

  const bars = data.map((v, i) => {
    const h = innerH * ((v - lo) / span);
    const x = PAD.left + i * slot + (slot - barW) / 2;
    const y = PAD.top + innerH - h;
    return `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${barW.toFixed(1)}" height="${Math.max(h, 1).toFixed(1)}" rx="3" fill="${color}"/>`;
  }).join('');

  const xLabels = labels.length
    ? labels.map((l, i) => {
        const x = PAD.left + i * slot + slot / 2;
        return `<text x="${x.toFixed(1)}" y="${H - 6}" text-anchor="middle" font-size="10" fill="var(--chart-text)">${l}</text>`;
      }).join('')
    : '';

  return `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" style="width:100%;height:100%;display:block">
    ${gridLines(domain)}
    ${bars}
    ${xLabels}
  </svg>`;
}
