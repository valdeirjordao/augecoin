// AUGECOIN Wallet — smart-contract payload encoders (Fase 19 / integração carteira).
//
// Espelha byte-a-byte crates/augecoin-contracts/src/codec.rs (e o TS SDK) para
// que os payloads de instantiação/execução de AUGE20 sejam wire-compatíveis com
// o runtime WASM do nó. Não altere o layout de bytes.

import { bytesToHex, hexToBytes } from './utils.js';

/** Encode a u64 as 8-byte big-endian hex (account/code/contract ids). */
export function u64ToBeHex(value) {
  const v = BigInt(value);
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, v, false);
  return bytesToHex(bytes);
}

/** AUGE20 TokenInit -> payload de instantiação (hex-ready). */
export function encodeTokenInit(init) {
  const out = [];
  const push = (arr) => out.push(...arr);
  const u8 = (v) => push([v & 0xff]);
  const u32 = (v) => {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, Number(v) >>> 0, false);
    push(Array.from(b));
  };
  const u64 = (v) => push(Array.from(hexToBytes(u64ToBeHex(v))));
  const str = (s) => {
    const enc = new TextEncoder().encode(s);
    u32(enc.length);
    push(Array.from(enc));
  };

  u8(init.decimals & 0xff);
  u8(init.mintEnabled ? 1 : 0);
  u8(init.burnEnabled ? 1 : 0);
  u64(init.initialSupply);
  u64(init.maxSupply);
  str(init.name);
  str(init.symbol);
  return new Uint8Array(out);
}

/** AUGE20 TokenCall -> payload de execução (hex-ready). */
export function encodeTokenCall(call) {
  const out = [];
  const push = (arr) => out.push(...arr);
  const u8 = (v) => push([v & 0xff]);
  const u64 = (v) => push(Array.from(hexToBytes(u64ToBeHex(v))));

  if (call.kind === 'transfer') {
    u8(0);
    u64(call.to);
    u64(call.amount);
  } else if (call.kind === 'mint') {
    u8(1);
    u64(call.to);
    u64(call.amount);
  } else {
    u8(2);
    u64(call.amount);
  }
  return new Uint8Array(out);
}

/** Code id do AUGE20 built-in (AUGE20_CODE_ID = 1 no runtime). */
export const AUGE20_CODE_ID = 1n;

/** ContractOp::CreateContract (op_type 1) serializado pelo codec Rust. */
export function encodeCreateContractData(codeId, instantiateData) {
  const out = new Uint8Array(1 + 8 + 4 + instantiateData.length);
  const view = new DataView(out.buffer);
  out[0] = 1; // ContractOp::CreateContract
  view.setBigUint64(1, BigInt(codeId), false);
  view.setUint32(9, instantiateData.length, false);
  out.set(instantiateData, 13);
  return out;
}

/**
 * ContractOp::ExecuteContract (op_type 2) -> bytes `data` da operação.
 * Espelha crates/augecoin-contracts/src/lib.rs: contract_id (32 bytes fixos)
 * + call_data (u32-len prefixado).
 */
export function encodeExecuteContractData(contractIdHex, callData) {
  const id = hexToBytes(String(contractIdHex).trim());
  if (id.length !== 32) throw new Error('contract id deve ter 32 bytes (64 hex).');
  const out = new Uint8Array(1 + 32 + 4 + callData.length);
  const view = new DataView(out.buffer);
  out[0] = 2; // ContractOp::ExecuteContract
  out.set(id, 1);
  view.setUint32(33, callData.length, false);
  out.set(callData, 37);
  return out;
}
