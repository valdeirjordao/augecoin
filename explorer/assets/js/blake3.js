// Self-contained BLAKE3 (XOF) for AUGECOIN address derivation.
//
// Only the single-block path (input <= 64 bytes) is implemented, which is
// exactly what `derive_address` needs (a 32-byte Ed25519 public key).
// Verified against crates/augecoin-crypto/src/address.rs via the cross-language
// vector in crates/augecoin-node/tests/sdk_parity.rs.

const IV = new Uint32Array([
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
  0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
]);

// blake3 message permutation schedule (7 rounds, cumulative).
const MSG_PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];
const SIGMA = (() => {
  let v = Array.from({ length: 16 }, (_, i) => i);
  const res = [];
  for (let i = 0; i < 7; i++) {
    res.push(...v);
    v = MSG_PERM.map((idx) => v[idx]);
  }
  return new Uint8Array(res);
})();

const rotr = (x, n) => ((x >>> n) | (x << (32 - n))) >>> 0;

function g(state, a, b, c, d, x, y) {
  state[a] = (state[a] + state[b] + x) >>> 0;
  state[d] = rotr(state[d] ^ state[a], 16);
  state[c] = (state[c] + state[d]) >>> 0;
  state[b] = rotr(state[b] ^ state[c], 12);
  state[a] = (state[a] + state[b] + y) >>> 0;
  state[d] = rotr(state[d] ^ state[a], 8);
  state[c] = (state[c] + state[d]) >>> 0;
  state[b] = rotr(state[b] ^ state[c], 7);
}

function round(state, m) {
  g(state, 0, 4, 8, 12, m[0], m[1]);
  g(state, 1, 5, 9, 13, m[2], m[3]);
  g(state, 2, 6, 10, 14, m[4], m[5]);
  g(state, 3, 7, 11, 15, m[6], m[7]);
  g(state, 0, 5, 10, 15, m[8], m[9]);
  g(state, 1, 6, 11, 12, m[10], m[11]);
  g(state, 2, 7, 8, 13, m[12], m[13]);
  g(state, 3, 4, 9, 14, m[14], m[15]);
}

/**
 * BLAKE3-512 (XOF 64 bytes) of a single-block message (<= 64 bytes).
 * @param {Uint8Array} data
 * @returns {Uint8Array} 64 bytes
 */
export function blake3_512(data) {
  if (data.length > 64) throw new Error('blake3_512: single-block only (<= 64 bytes)');

  const block = new Uint8Array(64);
  block.set(data);

  const m = new Uint32Array(16);
  for (let i = 0; i < 16; i++) {
    m[i] = (block[i * 4] | (block[i * 4 + 1] << 8) | (block[i * 4 + 2] << 16) | (block[i * 4 + 3] << 24)) >>> 0;
  }

  // flags = CHUNK_START | CHUNK_END | ROOT = 1 | 2 | 8 = 11
  const state = new Uint32Array(16);
  state[0] = IV[0]; state[1] = IV[1]; state[2] = IV[2]; state[3] = IV[3];
  state[4] = IV[4]; state[5] = IV[5]; state[6] = IV[6]; state[7] = IV[7];
  state[8] = IV[0]; state[9] = IV[1]; state[10] = IV[2]; state[11] = IV[3];
  state[12] = 0;          // counter low
  state[13] = 0;          // counter high
  state[14] = data.length; // block length
  state[15] = 11;         // flags

  const v = state.slice();
  for (let r = 0; r < 7; r++) {
    const perm = SIGMA.subarray(r * 16, r * 16 + 16);
    const mm = new Uint32Array(16);
    for (let i = 0; i < 16; i++) mm[i] = m[perm[i]];
    round(v, mm);
  }

  const out = new Uint8Array(64);
  const out32 = new Uint32Array(16);
  for (let i = 0; i < 8; i++) out32[i] = (v[i] ^ v[i + 8]) >>> 0;
  for (let i = 0; i < 8; i++) out32[8 + i] = (IV[i] ^ v[i + 8]) >>> 0;

  for (let i = 0; i < 16; i++) {
    const w = out32[i];
    out[i * 4] = w & 0xff;
    out[i * 4 + 1] = (w >>> 8) & 0xff;
    out[i * 4 + 2] = (w >>> 16) & 0xff;
    out[i * 4 + 3] = (w >>> 24) & 0xff;
  }
  return out;
}
