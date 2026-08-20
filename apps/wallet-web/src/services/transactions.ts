// AUGECOIN Wallet Web — operation serialization.
//
// Mirrors crates/augecoin-core/src/operation.rs byte-for-byte (big-endian,
// u16 byte-string lengths, 64-byte Ed25519 signatures). Only the operation
// types reachable from the wallet UI are implemented here.

export type OperationPayload =
  | {
      type: 'transaction';
      senders: SenderInfo[];
      receivers: ReceiverInfo[];
      changers: ChangerInfo[];
      fee: bigint;
    }
  | {
      type: 'list_account_for_sale';
      account: bigint;
      n_operation: bigint;
      sale_price: bigint;
      account_to_pay: bigint;
      new_ed25519_public_key: Uint8Array;
      locked_until_block: bigint;
      fee: bigint;
    }
  | { type: 'delist_account'; account: bigint; n_operation: bigint; fee: bigint }
  | {
      type: 'buy_account';
      buyer_account: bigint;
      n_operation: bigint;
      account_to_purchase: bigint;
      amount: bigint;
      fee: bigint;
      new_ed25519_public_key: Uint8Array;
      seller_account: bigint;
    }
  | {
      type: 'gift_account';
      account: bigint;
      n_operation: bigint;
      recipient_public_key: Uint8Array;
      fee: bigint;
    }
  | { type: 'accept_gift'; account: bigint; n_operation: bigint; fee: bigint }
  | {
      type: 'change_key';
      account: bigint;
      n_operation: bigint;
      fee: bigint;
      new_ed25519_public_key: Uint8Array;
    }
  | {
      type: 'change_account_info';
      account: bigint;
      n_operation: bigint;
      fee: bigint;
      new_ed25519_public_key: Uint8Array;
      new_name: string | null;
      new_type: number;
      new_account_data: Uint8Array;
      new_account_seal: Uint8Array;
    };

export interface SenderInfo {
  account: bigint;
  n_operation: bigint;
  amount: bigint;
  payload: Uint8Array;
}

export interface ReceiverInfo {
  account: bigint;
  amount: bigint;
  payload: Uint8Array;
}

export interface ChangerInfo {
  account: bigint;
  n_operation: bigint;
  new_ed25519_public_key: Uint8Array;
  new_name: string | null;
  new_type: number;
  new_account_data: Uint8Array;
  new_account_seal: Uint8Array;
  fee: bigint;
}

export interface Operation {
  chainId: bigint;
  payload: OperationPayload;
}

const OP_TAG = {
  transaction: 0x01,
  change_key: 0x02,
  list_account_for_sale: 0x04,
  delist_account: 0x05,
  buy_account: 0x06,
  change_account_info: 0x08,
  gift_account: 0x0d,
  accept_gift: 0x0e,
} as const;

class Writer {
  private chunks: Uint8Array[] = [];
  private len = 0;

  u8(v: number): void {
    const b = new Uint8Array(1);
    b[0] = v & 0xff;
    this.chunks.push(b);
    this.len += 1;
  }

  u16(v: number): void {
    const b = new Uint8Array(2);
    new DataView(b.buffer).setUint16(0, v, false);
    this.chunks.push(b);
    this.len += 2;
  }

  u32(v: number): void {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v, false);
    this.chunks.push(b);
    this.len += 4;
  }

  u64(v: bigint): void {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, v, false);
    this.chunks.push(b);
    this.len += 8;
  }

  fixed(data: Uint8Array): void {
    this.chunks.push(data);
    this.len += data.length;
  }

  bytes(data: Uint8Array): void {
    this.u16(data.length);
    this.fixed(data);
  }

  optionalStr(s: string | null): void {
    if (s === null) {
      this.u16(0xffff);
    } else {
      this.bytes(new TextEncoder().encode(s));
    }
  }

  finish(): Uint8Array {
    const out = new Uint8Array(this.len);
    let o = 0;
    for (const c of this.chunks) {
      out.set(c, o);
      o += c.length;
    }
    return out;
  }
}

function writeSender(w: Writer, s: SenderInfo): void {
  w.u64(s.account);
  w.u64(s.n_operation);
  w.u64(s.amount);
  w.bytes(s.payload);
}

function writeReceiver(w: Writer, r: ReceiverInfo): void {
  w.u64(r.account);
  w.u64(r.amount);
  w.bytes(r.payload);
}

function writeChanger(w: Writer, c: ChangerInfo): void {
  w.u64(c.account);
  w.u64(c.n_operation);
  w.fixed(c.new_ed25519_public_key);
  if (c.new_name === null) {
    w.u16(0xffff);
  } else {
    w.bytes(new TextEncoder().encode(c.new_name));
  }
  w.u16(c.new_type);
  w.bytes(c.new_account_data);
  w.bytes(c.new_account_seal);
  w.u64(c.fee);
}

function writePayload(w: Writer, p: OperationPayload): void {
  switch (p.type) {
    case 'transaction':
      w.u8(OP_TAG.transaction);
      w.u32(p.senders.length);
      for (const s of p.senders) writeSender(w, s);
      w.u32(p.receivers.length);
      for (const r of p.receivers) writeReceiver(w, r);
      w.u32(p.changers.length);
      for (const c of p.changers) writeChanger(w, c);
      w.u64(p.fee);
      break;
    case 'list_account_for_sale':
      w.u8(OP_TAG.list_account_for_sale);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.sale_price);
      w.u64(p.account_to_pay);
      w.fixed(p.new_ed25519_public_key);
      w.u64(p.locked_until_block);
      w.u64(p.fee);
      break;
    case 'delist_account':
      w.u8(OP_TAG.delist_account);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      break;
    case 'buy_account':
      w.u8(OP_TAG.buy_account);
      w.u64(p.buyer_account);
      w.u64(p.n_operation);
      w.u64(p.account_to_purchase);
      w.u64(p.amount);
      w.u64(p.fee);
      w.fixed(p.new_ed25519_public_key);
      w.u64(p.seller_account);
      break;
    case 'gift_account':
      w.u8(OP_TAG.gift_account);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.fixed(p.recipient_public_key);
      w.u64(p.fee);
      break;
    case 'accept_gift':
      w.u8(OP_TAG.accept_gift);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      break;
    case 'change_key':
      w.u8(OP_TAG.change_key);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      w.fixed(p.new_ed25519_public_key);
      break;
    case 'change_account_info':
      w.u8(OP_TAG.change_account_info);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      w.fixed(p.new_ed25519_public_key);
      w.optionalStr(p.new_name);
      w.u16(p.new_type);
      w.bytes(p.new_account_data);
      w.bytes(p.new_account_seal);
      break;
  }
}

/** Serialize payload + chain_id — the exact bytes that get signed. */
export function serializeOperationStripped(op: Operation): Uint8Array {
  const w = new Writer();
  writePayload(w, op.payload);
  w.u64(op.chainId);
  return w.finish();
}

/** Full signed operation: stripped + u32(sig count) + signatures. */
export function serializeSignedOperation(op: Operation, signatures: Uint8Array[]): Uint8Array {
  const w = new Writer();
  writePayload(w, op.payload);
  w.u64(op.chainId);
  w.u32(signatures.length);
  for (const s of signatures) w.fixed(s);
  return w.finish();
}

function toBig(v: number | bigint): bigint {
  return typeof v === 'bigint' ? v : BigInt(Math.trunc(v));
}

export function createTransfer(
  chainId: number | bigint,
  sender: number,
  nOperation: number,
  to: number,
  amount: number,
  fee: number,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: {
      type: 'transaction',
      senders: [{ account: toBig(sender), n_operation: toBig(nOperation), amount: toBig(amount), payload: new Uint8Array() }],
      receivers: [{ account: toBig(to), amount: toBig(amount), payload: new Uint8Array() }],
      changers: [],
      fee: toBig(fee),
    },
  };
}

export function createListAccountForSale(
  chainId: number | bigint,
  account: number,
  nOperation: number,
  salePrice: number,
  accountToPay: number,
  newPublicKey: Uint8Array,
  lockedUntilBlock: number,
  fee: number,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: {
      type: 'list_account_for_sale',
      account: toBig(account),
      n_operation: toBig(nOperation),
      sale_price: toBig(salePrice),
      account_to_pay: toBig(accountToPay),
      new_ed25519_public_key: newPublicKey,
      locked_until_block: toBig(lockedUntilBlock),
      fee: toBig(fee),
    },
  };
}

export function createDelistAccount(
  chainId: number | bigint,
  account: number,
  nOperation: number,
  fee: number,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: { type: 'delist_account', account: toBig(account), n_operation: toBig(nOperation), fee: toBig(fee) },
  };
}

export function createBuyAccount(
  chainId: number | bigint,
  buyerAccount: number,
  nOperation: number,
  accountToPurchase: number,
  amount: number,
  fee: number,
  newPublicKey: Uint8Array,
  sellerAccount: number,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: {
      type: 'buy_account',
      buyer_account: toBig(buyerAccount),
      n_operation: toBig(nOperation),
      account_to_purchase: toBig(accountToPurchase),
      amount: toBig(amount),
      fee: toBig(fee),
      new_ed25519_public_key: newPublicKey,
      seller_account: toBig(sellerAccount),
    },
  };
}

export function createGiftAccount(
  chainId: number | bigint,
  account: number,
  nOperation: number,
  recipientPublicKey: Uint8Array,
  fee: number,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: {
      type: 'gift_account',
      account: toBig(account),
      n_operation: toBig(nOperation),
      recipient_public_key: recipientPublicKey,
      fee: toBig(fee),
    },
  };
}

export function createAcceptGift(
  chainId: number | bigint,
  account: number,
  nOperation: number,
  fee: number,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: { type: 'accept_gift', account: toBig(account), n_operation: toBig(nOperation), fee: toBig(fee) },
  };
}

export function createChangeKey(
  chainId: number | bigint,
  account: number,
  nOperation: number,
  fee: number,
  newPublicKey: Uint8Array,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: {
      type: 'change_key',
      account: toBig(account),
      n_operation: toBig(nOperation),
      fee: toBig(fee),
      new_ed25519_public_key: newPublicKey,
    },
  };
}

export function createChangeAccountInfo(
  chainId: number | bigint,
  account: number,
  nOperation: number,
  fee: number,
  newPublicKey: Uint8Array,
  newName: string | null,
  newType: number,
  newAccountData: Uint8Array,
  newAccountSeal: Uint8Array,
): Operation {
  return {
    chainId: toBig(chainId),
    payload: {
      type: 'change_account_info',
      account: toBig(account),
      n_operation: toBig(nOperation),
      fee: toBig(fee),
      new_ed25519_public_key: newPublicKey,
      new_name: newName,
      new_type: newType,
      new_account_data: newAccountData,
      new_account_seal: newAccountSeal,
    },
  };
}

export function hexToBytes(hex: string): Uint8Array {
  const clean = String(hex).replace(/^0x/, '').replace(/\s+/g, '');
  if (clean.length % 2 !== 0) throw new Error('invalid hex string');
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.substr(i * 2, 2), 16);
  return out;
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}
