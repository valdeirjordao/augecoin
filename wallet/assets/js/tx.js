// AUGECOIN Wallet — operation serialization (mirrors crates/augecoin-core/
// src/operation.rs byte-for-byte).

const OP_TAG = {
  transaction: 0x01,
  change_key: 0x02,
  list_account_for_sale: 0x04,
  delist_account: 0x05,
  buy_account: 0x06,
  change_account_info: 0x08,
  multi_operation: 0x09,
  gift_account: 0x0d,
  accept_gift: 0x0e,
};

class Writer {
  constructor() { this.chunks = []; this.len = 0; }
  u8(v) { const b = new Uint8Array(1); b[0] = v & 0xff; this.chunks.push(b); this.len += 1; }
  u16(v) { const b = new Uint8Array(2); new DataView(b.buffer).setUint16(0, v, false); this.chunks.push(b); this.len += 2; }
  u32(v) { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, false); this.chunks.push(b); this.len += 4; }
  u64(v) { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), false); this.chunks.push(b); this.len += 8; }
  fixed(data) { this.chunks.push(data); this.len += data.length; }
  bytes(data) { this.u16(data.length); this.fixed(data); }
  optionalStr(s) { if (s === null || s === undefined) this.u16(0xffff); else this.bytes(new TextEncoder().encode(s)); }
  finish() {
    const out = new Uint8Array(this.len);
    let o = 0;
    for (const c of this.chunks) { out.set(c, o); o += c.length; }
    return out;
  }
}

function writeSender(w, s) {
  w.u64(s.account); w.u64(s.n_operation); w.u64(s.amount); w.bytes(s.payload || new Uint8Array());
}
function writeReceiver(w, r) {
  w.u64(r.account); w.u64(r.amount); w.bytes(r.payload || new Uint8Array());
}
function writeSendersReceiversChangers(w, senders, receivers, changers) {
  w.u32(senders.length);
  for (const s of senders) writeSender(w, s);
  w.u32(receivers.length);
  for (const r of receivers) writeReceiver(w, r);
  w.u32(changers.length);
}

function writePayload(w, payload) {
  switch (payload.type) {
    case 'transaction':
      w.u8(OP_TAG.transaction);
      writeSendersReceiversChangers(w, payload.senders, payload.receivers, payload.changers || []);
      w.u64(payload.fee);
      break;
    case 'multi_operation':
      w.u8(OP_TAG.multi_operation);
      writeSendersReceiversChangers(w, payload.senders, payload.receivers, payload.changers || []);
      w.u64(payload.fee);
      break;
    case 'list_account_for_sale':
      w.u8(OP_TAG.list_account_for_sale);
      w.u64(payload.account);
      w.u64(payload.n_operation);
      w.u64(payload.sale_price);
      w.u64(payload.account_to_pay);
      w.fixed(payload.new_ed25519_public_key);
      w.u64(payload.locked_until_block);
      w.u64(payload.fee);
      break;
    case 'delist_account':
      w.u8(OP_TAG.delist_account);
      w.u64(payload.account);
      w.u64(payload.n_operation);
      w.u64(payload.fee);
      break;
    case 'buy_account':
      w.u8(OP_TAG.buy_account);
      w.u64(payload.buyer_account);
      w.u64(payload.n_operation);
      w.u64(payload.account_to_purchase);
      w.u64(payload.amount);
      w.u64(payload.fee);
      w.fixed(payload.new_ed25519_public_key);
      w.u64(payload.seller_account);
      break;
    case 'change_key':
      w.u8(OP_TAG.change_key);
      w.u64(payload.account);
      w.u64(payload.n_operation);
      w.u64(payload.fee);
      w.fixed(payload.new_ed25519_public_key);
      break;
    case 'change_account_info':
      w.u8(OP_TAG.change_account_info);
      w.u64(payload.account);
      w.u64(payload.n_operation);
      w.u64(payload.fee);
      w.fixed(payload.new_ed25519_public_key);
      w.optionalStr(payload.new_name);
      w.u16(payload.new_type);
      w.bytes(payload.new_account_data || new Uint8Array());
      w.bytes(payload.new_account_seal || new Uint8Array());
      break;
    case 'gift_account':
      w.u8(OP_TAG.gift_account);
      w.u64(payload.account);
      w.u64(payload.n_operation);
      w.fixed(payload.recipient_public_key);
      w.u64(payload.fee);
      break;
    case 'accept_gift':
      w.u8(OP_TAG.accept_gift);
      w.u64(payload.account);
      w.u64(payload.n_operation);
      w.u64(payload.fee);
      break;
    default:
      throw new Error('unsupported payload type: ' + payload.type);
  }
}

/** Serialize payload + chain_id. This is the message that gets signed. */
export function serializeOperationStripped(op) {
  const w = new Writer();
  writePayload(w, op.payload);
  w.u64(op.chainId);
  return w.finish();
}

/** stripped bytes + u32(signature count) + signatures. */
export function serializeSignedOperation(op, signatures) {
  const w = new Writer();
  writePayload(w, op.payload);
  w.u64(op.chainId);
  w.u32(signatures.length);
  for (const sig of signatures) w.fixed(sig);
  return w.finish();
}

/** Build a single-sender → single-receiver transfer operation. */
export function createTransfer(chainId, sender, nOperation, to, amount, fee) {
  return {
    chainId,
    payload: {
      type: 'transaction',
      senders: [{ account: sender, n_operation: nOperation, amount, payload: new Uint8Array() }],
      receivers: [{ account: to, amount, payload: new Uint8Array() }],
      changers: [],
      fee,
    },
  };
}

export function createListAccountForSale(chainId, account, nOperation, salePrice, accountToPay, newEd25519PublicKey, lockedUntilBlock, fee) {
  return {
    chainId,
    payload: {
      type: 'list_account_for_sale',
      account, n_operation: nOperation, sale_price: salePrice, account_to_pay: accountToPay,
      new_ed25519_public_key: newEd25519PublicKey, locked_until_block: lockedUntilBlock, fee,
    },
  };
}

export function createDelistAccount(chainId, account, nOperation, fee) {
  return { chainId, payload: { type: 'delist_account', account, n_operation: nOperation, fee } };
}

export function createBuyAccount(chainId, buyerAccount, nOperation, accountToPurchase, amount, fee, newEd25519PublicKey, sellerAccount) {
  return {
    chainId,
    payload: {
      type: 'buy_account',
      buyer_account: buyerAccount, n_operation: nOperation, account_to_purchase: accountToPurchase,
      amount, fee, new_ed25519_public_key: newEd25519PublicKey, seller_account: sellerAccount,
    },
  };
}

export function createChangeKey(chainId, account, nOperation, fee, newEd25519PublicKey) {
  return {
    chainId,
    payload: { type: 'change_key', account, n_operation: nOperation, fee, new_ed25519_public_key: newEd25519PublicKey },
  };
}

export function createChangeAccountInfo(chainId, account, nOperation, fee, newEd25519PublicKey, newName, newType, newAccountData, newAccountSeal) {
  return {
    chainId,
    payload: {
      type: 'change_account_info',
      account, n_operation: nOperation, fee, new_ed25519_public_key: newEd25519PublicKey,
      new_name: newName, new_type: newType, new_account_data: newAccountData, new_account_seal: newAccountSeal,
    },
  };
}

export function createGiftAccount(chainId, account, nOperation, recipientPublicKey, fee) {
  return {
    chainId,
    payload: { type: 'gift_account', account, n_operation: nOperation, recipient_public_key: recipientPublicKey, fee },
  };
}

export function createAcceptGift(chainId, account, nOperation, fee) {
  return { chainId, payload: { type: 'accept_gift', account, n_operation: nOperation, fee } };
}
