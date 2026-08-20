// AUGECOIN Wallet — minimal QR code encoder (byte mode, EC level L).
// Self-contained, no dependencies. Returns a 2D boolean matrix (true = dark).

const EC_LEVEL = 'L';

// Number of error correction codewords per block, per version (EC L).
const ECC_CODEWORDS_L = [
  -1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
];

// Number of blocks in group 1/2 and codewords per block, per version (EC L).
const BLOCKS_L = [
  null,
  { g1: 1, c1: 19 }, { g1: 1, c1: 34 }, { g1: 1, c1: 55 }, { g1: 1, c1: 80 }, { g1: 1, c1: 108 },
  { g1: 2, c1: 68 }, { g1: 2, c1: 78 }, { g1: 2, c1: 97 }, { g1: 2, c1: 116 }, { g1: 2, c1: 68, g2: 2, c2: 69 },
  { g1: 4, c1: 81 }, { g1: 2, c1: 92, g2: 2, c2: 93 }, { g1: 4, c1: 107 }, { g1: 3, c1: 115, g2: 1, c2: 116 },
  { g1: 5, c1: 87, g2: 1, c2: 88 }, { g1: 5, c1: 98, g2: 1, c2: 99 }, { g1: 1, c1: 107, g2: 5, c2: 108 },
  { g1: 5, c1: 120, g2: 1, c2: 121 }, { g1: 3, c1: 113, g2: 4, c2: 114 }, { g1: 3, c1: 107, g2: 5, c2: 108 },
  { g1: 4, c1: 116, g2: 4, c2: 117 }, { g1: 2, c1: 111, g2: 7, c2: 112 }, { g1: 4, c1: 121, g2: 5, c2: 122 },
  { g1: 6, c1: 117, g2: 4, c2: 118 }, { g1: 8, c1: 106, g2: 4, c2: 107 }, { g1: 10, c1: 114, g2: 2, c2: 115 },
  { g1: 8, c1: 122, g2: 4, c2: 123 }, { g1: 3, c1: 117, g2: 10, c2: 118 }, { g1: 7, c1: 116, g2: 7, c2: 117 },
  { g1: 5, c1: 115, g2: 10, c2: 116 }, { g1: 13, c1: 115, g2: 3, c2: 116 }, { g1: 17, c1: 115 }, { g1: 17, c1: 115, g2: 1, c2: 116 },
  { g1: 13, c1: 115, g2: 6, c2: 116 }, { g1: 12, c1: 121, g2: 7, c2: 122 }, { g1: 6, c1: 121, g2: 14, c2: 122 },
  { g1: 17, c1: 122, g2: 4, c2: 123 }, { g1: 4, c1: 122, g2: 18, c2: 123 }, { g1: 20, c1: 117, g2: 4, c2: 118 },
  { g1: 19, c1: 118, g2: 6, c2: 119 },
];

// ── GF(256) + Reed-Solomon ─────────────────────────────────────────────

const EXP = new Int32Array(512);
const LOG = new Int32Array(256);
{
  let x = 1;
  for (let i = 0; i < 255; i++) {
    EXP[i] = x; LOG[x] = i;
    x <<= 1;
    if (x & 0x100) x ^= 0x11d;
  }
  for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255];
}

function rsGenerator(degree) {
  let gen = new Int32Array([1]);
  for (let i = 0; i < degree; i++) {
    const next = new Int32Array(gen.length + 1);
    for (let j = 0; j < gen.length; j++) {
      next[j] ^= gen[j];                    // x·gen term (degree shift)
      next[j + 1] ^= gfMul(gen[j], EXP[i]); // (x + α^i) constant term
    }
    gen = next;
  }
  return gen;
}
function gfMul(a, b) { if (a === 0 || b === 0) return 0; return EXP[LOG[a] + LOG[b]]; }

function rsRemainder(data, degree, generator) {
  const res = new Int32Array(degree);
  for (const b of data) {
    const factor = b ^ res[0];
    res.copyWithin(0, 1);
    res[degree - 1] = 0;
    for (let i = 0; i < degree; i++) res[i] ^= gfMul(generator[i + 1], factor);
  }
  return res;
}

// ── Encoder ────────────────────────────────────────────────────────────

export function qrMatrix(text) {
  const bytes = new TextEncoder().encode(text);
  const version = chooseVersion(bytes.length);
  const size = version * 4 + 17;

  // Build the data bit stream (byte mode).
  const totalBits = totalCodewords(version) * 8;
  const bits = [];
  const pushBits = (val, len) => { for (let i = len - 1; i >= 0; i--) bits.push((val >> i) & 1); };
  pushBits(0b0100, 4);                          // byte mode
  pushBits(bytes.length, version < 10 ? 8 : 16); // char count
  for (const b of bytes) pushBits(b, 8);        // data
  for (let i = 0; i < Math.min(4, totalBits - bits.length); i++) bits.push(0); // terminator
  while (bits.length % 8 !== 0) bits.push(0);   // pad to byte boundary
  let padByte = 0xEC;
  while (bits.length < totalBits) {             // pad bytes
    for (let i = 7; i >= 0 && bits.length < totalBits; i--) bits.push((padByte >> i) & 1);
    padByte = padByte === 0xEC ? 0x11 : 0xEC;
  }

  const dataCodewords = [];
  for (let i = 0; i < totalBits; i += 8) {
    let v = 0;
    for (let j = 0; j < 8; j++) v = (v << 1) | bits[i + j];
    dataCodewords.push(v);
  }

  // Split into blocks and interleave.
  const codewords = interleave(version, dataCodewords);

  // Build modules. Values: 2 = function-dark, 3 = function-light, 0/1 = data.
  const matrix = Array.from({ length: size }, () => new Uint8Array(size));
  drawFunctionPatterns(matrix, version);
  drawCodewords(matrix, codewords);

  // Auto-select best mask.
  let best = null, bestPenalty = Infinity;
  for (let mask = 0; mask < 8; mask++) {
    const m = applyMask(matrix, mask);
    drawFormat(m, formatBits(EC_LEVEL, mask));
    if (version >= 7) drawVersion(m, version);
    normalize(m);
    const p = penalty(m);
    if (p < bestPenalty) { bestPenalty = p; best = m; }
  }

  // Convert to booleans (dark = true).
  return best.map((row) => Array.from(row, (v) => v === 1));
}

function chooseVersion(len) {
  // byte-mode capacity (EC L) per version
  const cap = [0, 17, 32, 53, 78, 106, 134, 154, 192, 230, 271, 321, 367, 425, 458, 520, 586, 644, 718, 792, 858, 929, 1003, 1091, 1171, 1273, 1367, 1465, 1528, 1628, 1732, 1840, 1952, 2068, 2188, 2303, 2431, 2563, 2699, 2809, 2953];
  for (let v = 1; v <= 40; v++) if (cap[v] >= len) return v;
  throw new Error('data too long for QR');
}

function totalCodewords(version) {
  const b = BLOCKS_L[version];
  let total = b.g1 * b.c1;
  if (b.g2) total += b.g2 * b.c2;
  return total;
}

function interleave(version, data) {
  const b = BLOCKS_L[version];
  const ecPerBlock = ECC_CODEWORDS_L[version];
  const gen = rsGenerator(ecPerBlock);
  const blocks = [];
  let offset = 0;
  for (let i = 0; i < b.g1; i++) { blocks.push({ data: data.slice(offset, offset + b.c1), ec: null }); offset += b.c1; }
  for (let i = 0; i < (b.g2 || 0); i++) { blocks.push({ data: data.slice(offset, offset + b.c2), ec: null }); offset += b.c2; }
  for (const blk of blocks) blk.ec = rsRemainder(blk.data, ecPerBlock, gen);

  const out = [];
  const maxData = Math.max(b.c1, b.c2 || 0);
  for (let i = 0; i < maxData; i++) for (const blk of blocks) if (i < blk.data.length) out.push(blk.data[i]);
  for (let i = 0; i < ecPerBlock; i++) for (const blk of blocks) out.push(blk.ec[i]);
  return out;
}

function drawFunctionPatterns(m, version) {
  const size = m.length;
  const dark = 2, light = 3;
  // finder patterns + separators
  for (const [r, c] of [[0, 0], [0, size - 7], [size - 7, 0]]) {
    for (let i = -1; i <= 7; i++) for (let j = -1; j <= 7; j++) {
      const rr = r + i, cc = c + j;
      if (rr < 0 || cc < 0 || rr >= size || cc >= size) continue;
      const inFinder = i >= 0 && i <= 6 && j >= 0 && j <= 6;
      const d = inFinder && (i === 0 || i === 6 || j === 0 || j === 6 || (i >= 2 && i <= 4 && j >= 2 && j <= 4));
      m[rr][cc] = d ? dark : light;
    }
  }
  // timing patterns
  for (let i = 8; i < size - 8; i++) {
    m[6][i] = m[i][6] = (i % 2 === 0) ? dark : light;
  }
  // alignment patterns
  const centers = alignmentCenters(version);
  for (const r of centers) for (const c of centers) {
    if ((r <= 8 && c <= 8) || (r <= 8 && c >= size - 9) || (r >= size - 9 && c <= 8)) continue;
    for (let i = -2; i <= 2; i++) for (let j = -2; j <= 2; j++) {
      const d = Math.max(Math.abs(i), Math.abs(j)) !== 1;
      m[r + i][c + j] = d ? dark : light;
    }
  }
  // dark module + reserve format/version areas
  m[size - 8][8] = dark;
  for (let i = 0; i < 9; i++) {
    if (m[8][i] === 0) m[8][i] = light;
    if (m[i][8] === 0) m[i][8] = light;
  }
  for (let i = 0; i < 8; i++) {
    if (m[8][size - 1 - i] === 0) m[8][size - 1 - i] = light;
    if (m[size - 1 - i][8] === 0) m[size - 1 - i][8] = light;
  }
  if (version >= 7) {
    for (let i = 0; i < 6; i++) for (let j = 0; j < 3; j++) {
      m[i][size - 11 + j] = light;
      m[size - 11 + j][i] = light;
    }
  }
}

function alignmentCenters(version) {
  if (version === 1) return [];
  const num = Math.floor(version / 7) + 2;
  const size = version * 4 + 17;
  const step = Math.ceil((size - 13) / (num * 2 - 2)) * 2;
  const out = [6];
  for (let i = size - 7; out.length < num; i -= step) out.push(i);
  return out;
}

function drawCodewords(m, codewords) {
  const size = m.length;
  let bit = 0;
  let up = true;
  for (let c = size - 1; c > 0; c -= 2) {
    if (c === 6) c--;
    for (let k = 0; k < size; k++) {
      const r = up ? size - 1 - k : k;
      for (let j = 0; j < 2; j++) {
        const cc = c - j;
        if (m[r][cc] === 2 || m[r][cc] === 3) continue; // function module
        let v = 0;
        if (bit < codewords.length * 8) v = (codewords[bit >> 3] >> (7 - (bit & 7))) & 1;
        bit++;
        m[r][cc] = v;
      }
    }
    up = !up;
  }
}

function applyMask(m, mask) {
  const size = m.length;
  const out = m.map((row) => row.slice());
  for (let r = 0; r < size; r++) for (let c = 0; c < size; c++) {
    if (out[r][c] === 2 || out[r][c] === 3) continue; // function module
    if (maskFn(mask, r, c)) out[r][c] ^= 1;
  }
  return out;
}

function normalize(m) {
  const size = m.length;
  for (let r = 0; r < size; r++) for (let c = 0; c < size; c++) {
    if (m[r][c] === 2) m[r][c] = 1;
    else if (m[r][c] === 3) m[r][c] = 0;
  }
}

function maskFn(mask, r, c) {
  switch (mask) {
    case 0: return (r + c) % 2 === 0;
    case 1: return r % 2 === 0;
    case 2: return c % 3 === 0;
    case 3: return (r + c) % 3 === 0;
    case 4: return (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0;
    case 5: return ((r * c) % 2) + ((r * c) % 3) === 0;
    case 6: return (((r * c) % 2) + ((r * c) % 3)) % 2 === 0;
    case 7: return (((r + c) % 2) + ((r * c) % 3)) % 2 === 0;
  }
  return false;
}

const FORMAT_L = { L: 1, M: 0, Q: 3, H: 2 };
function formatBits(level, mask) {
  const data = (FORMAT_L[level] << 3) | mask;
  let bits = data << 10;
  let rem = bits;
  const gen = 0b10100110111;
  for (let i = 14; i >= 10; i--) {
    if (rem & (1 << i)) rem ^= gen << (i - 10);
  }
  return ((data << 10) | rem) ^ 0b101010000010010;
}

function drawFormat(m, bits) {
  const size = m.length;
  const bit = (i) => (bits >> i) & 1;
  // First copy (vertical, column 8)
  for (let i = 0; i <= 5; i++) m[i][8] = bit(i);           // bits 0-5
  m[7][8] = bit(6);                                         // bit 6
  m[8][8] = bit(7);                                         // bit 7
  for (let i = 8; i <= 14; i++) m[size - 15 + i][8] = bit(i); // bits 8-14
  // Second copy (horizontal, row 8)
  for (let i = 0; i <= 7; i++) m[8][size - 1 - i] = bit(i);   // bits 0-7
  m[8][7] = bit(8);                                         // bit 8
  for (let i = 9; i <= 14; i++) m[8][14 - i] = bit(i);        // bits 9-14
  m[size - 8][8] = 1; // dark module
}

function drawVersion(m, version) {
  const size = m.length;
  const bits = versionBits(version);
  if (!bits) return;
  for (let i = 0; i < 18; i++) {
    const v = (bits >> i) & 1;
    const a = Math.floor(i / 3), b = (i % 3) + size - 11;
    m[a][b] = v; m[b][a] = v;
  }
}
function versionBits(version) {
  const gen = 0b1111100100101;
  let rem = version << 12;
  for (let i = 17; i >= 12; i--) if (rem & (1 << i)) rem ^= gen << (i - 12);
  return (version << 12) | rem;
}

function penalty(m) {
  const size = m.length;
  let p = 0;
  // rule 1: runs
  for (let r = 0; r < size; r++) {
    let run = 1;
    for (let c = 1; c < size; c++) { if (m[r][c] === m[r][c - 1]) run++; else { if (run >= 5) p += 3 + (run - 5); run = 1; } }
    if (run >= 5) p += 3 + (run - 5);
  }
  for (let c = 0; c < size; c++) {
    let run = 1;
    for (let r = 1; r < size; r++) { if (m[r][c] === m[r - 1][c]) run++; else { if (run >= 5) p += 3 + (run - 5); run = 1; } }
    if (run >= 5) p += 3 + (run - 5);
  }
  // rule 2: 2x2 blocks
  for (let r = 0; r < size - 1; r++) for (let c = 0; c < size - 1; c++) {
    if (m[r][c] === m[r][c + 1] && m[r][c] === m[r + 1][c] && m[r][c] === m[r + 1][c + 1]) p += 3;
  }
  // rule 3: finder-like patterns
  for (let r = 0; r < size; r++) for (let c = 0; c < size - 6; c++) {
    if (m[r][c] === 1 && m[r][c+1] === 0 && m[r][c+2] === 1 && m[r][c+3] === 1 && m[r][c+4] === 1 && m[r][c+5] === 0 && m[r][c+6] === 1) p += 40;
  }
  for (let c = 0; c < size; c++) for (let r = 0; r < size - 6; r++) {
    if (m[r][c] === 1 && m[r+1][c] === 0 && m[r+2][c] === 1 && m[r+3][c] === 1 && m[r+4][c] === 1 && m[r+5][c] === 0 && m[r+6][c] === 1) p += 40;
  }
  // rule 4: balance
  let dark = 0;
  for (let r = 0; r < size; r++) for (let c = 0; c < size; c++) dark += m[r][c];
  const total = size * size;
  const pct = (dark * 100) / total;
  p += Math.floor(Math.abs(pct - 50) / 5) * 10;
  return p;
}

export function qrToCanvas(matrix, canvas, scale = 4, margin = 4) {
  const n = matrix.length;
  const dim = (n + margin * 2) * scale;
  canvas.width = dim;
  canvas.height = dim;
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = '#ffffff';
  ctx.fillRect(0, 0, dim, dim);
  ctx.fillStyle = '#000000';
  for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) {
    if (matrix[r][c]) ctx.fillRect((c + margin) * scale, (r + margin) * scale, scale, scale);
  }
}
