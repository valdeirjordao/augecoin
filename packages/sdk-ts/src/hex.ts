/**
 * Hex and big-endian encoding helpers for the AUGECOIN SDK.
 *
 * Mirrors the deterministic, big-endian binary codec used by
 * crates/augecoin-contracts/src/codec.rs so that TypeScript callers produce
 * byte-for-byte identical payloads to the Rust node.
 */

export function bytesToHex(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += b.toString(16).padStart(2, '0');
  return s;
}

export function hexToBytes(hex: string): Uint8Array {
  const clean = hex.trim().replace(/^0x/i, '');
  if (clean.length % 2 !== 0) throw new Error('invalid hex: odd length');
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) {
    const byte = parseInt(clean.substr(i * 2, 2), 16);
    if (Number.isNaN(byte)) throw new Error('invalid hex: non-hex character');
    out[i] = byte;
  }
  return out;
}

/** Encode a u64 as 8-byte big-endian hex. */
export function u64ToBeHex(value: bigint | number): string {
  const v = BigInt(value);
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, v, false);
  return bytesToHex(bytes);
}

/** Decode 8-byte big-endian hex into a u64 (bigint). */
export function beHexToU64(hex: string): bigint {
  const bytes = hexToBytes(hex);
  if (bytes.length !== 8) throw new Error('expected 8-byte hex for u64');
  return new DataView(bytes.buffer).getBigUint64(0, false);
}

/** Encode an on-chain account number (Address, u64) as 8-byte big-endian hex. */
export function addressToBeHex(address: number): string {
  return u64ToBeHex(address);
}

/** Parse 8-byte big-endian hex into an account number. */
export function beHexToAddress(hex: string): number {
  return Number(beHexToU64(hex));
}

/** Encode a code id (u64) as 8-byte big-endian hex. */
export function codeIdToBeHex(codeId: bigint | number): string {
  return u64ToBeHex(codeId);
}

/** Parse 8-byte big-endian hex into a code id. */
export function beHexToCodeId(hex: string): bigint {
  return beHexToU64(hex);
}
