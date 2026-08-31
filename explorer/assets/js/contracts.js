import { blake3_512 } from './blake3.js';

function hexToBytes(hex) {
  const clean = String(hex || '').replace(/^0x/, '');
  if (!/^[0-9a-f]*$/i.test(clean) || clean.length % 2) return null;
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) bytes[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return bytes;
}

function bytesToHex(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function tokenAmount(value, decimals) {
  const unit = 10n ** BigInt(decimals);
  const whole = value / unit;
  const fraction = (value % unit).toString().padStart(decimals, '0').replace(/0+$/, '');
  return `${Number(whole).toLocaleString('en-US')}${fraction ? `.${fraction}` : ''}`;
}

/** Decode the V1 ContractOp payload used in on-chain operations. */
export function decodeContractOperation(payload) {
  if (!payload?.data_hex || Number(payload.op_type) !== 1) return null;
  const data = hexToBytes(payload.data_hex);
  if (!data || data.length < 13 || data[0] !== 1) return null;
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const codeId = view.getBigUint64(1, false);
  const length = view.getUint32(9, false);
  if (data.length !== 13 + length) return null;
  const init = data.slice(13);
  const result = { kind: 'create', codeId, contractId: null, token: null };

  // AUGE20 code id 1: decimals, mint flag, burn flag, supplies, name and symbol.
  if (codeId !== 1n || init.length < 23) return result;
  const tokenView = new DataView(init.buffer, init.byteOffset, init.byteLength);
  const decimals = init[0];
  const initialSupply = tokenView.getBigUint64(3, false);
  const maxSupply = tokenView.getBigUint64(11, false);
  const nameLength = tokenView.getUint32(19, false);
  const nameStart = 23;
  const symbolLengthOffset = nameStart + nameLength;
  if (symbolLengthOffset + 4 > init.length) return result;
  const symbolLength = tokenView.getUint32(symbolLengthOffset, false);
  const symbolStart = symbolLengthOffset + 4;
  if (symbolStart + symbolLength !== init.length) return result;
  const decoder = new TextDecoder();
  const name = decoder.decode(init.slice(nameStart, symbolLengthOffset));
  const symbol = decoder.decode(init.slice(symbolStart));
  const owner = BigInt(payload.account);
  // contract_id_of serializes init through Writer::bytes, including its u32 length.
  const idInput = new Uint8Array(20 + init.length);
  const idView = new DataView(idInput.buffer);
  idView.setBigUint64(0, owner, false);
  idView.setBigUint64(8, codeId, false);
  idView.setUint32(16, init.length, false);
  idInput.set(init, 20);
  result.contractId = bytesToHex(blake3_512(idInput).slice(0, 32));
  result.token = {
    decimals, name, symbol, initialSupply: tokenAmount(initialSupply, decimals),
    maxSupply: tokenAmount(maxSupply, decimals), mintEnabled: init[1] === 1, burnEnabled: init[2] === 1,
  };
  return result;
}
