/**
 * AUGECOIN transaction serialization.
 *
 * This module mirrors the Rust `Operation` serialization byte-for-byte
 * (crates/augecoin-core/src/operation.rs) so that the same logical operation
 * produces the same digest in Wallet, SDK and Node.
 *
 * Byte order: big-endian. Lengths for byte strings: u16. Signatures: 64-byte
 * Ed25519 (no ML-DSA — post-quantum layer is handled out-of-band).
 */

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
  new_ed25519_public_key: Uint8Array; // 32 bytes
  new_name: string | null;
  new_type: number; // u16
  new_account_data: Uint8Array;
  new_account_seal: Uint8Array;
  fee: bigint;
}

export type OperationPayload =
  | {
      type: 'transaction';
      senders: SenderInfo[];
      receivers: ReceiverInfo[];
      changers: ChangerInfo[];
      fee: bigint;
    }
  | {
      type: 'change_key';
      account: bigint;
      n_operation: bigint;
      fee: bigint;
      new_ed25519_public_key: Uint8Array;
    }
  | { type: 'recover_founds'; account: bigint }
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
      type: 'change_key_signed';
      account: bigint;
      n_operation: bigint;
      fee: bigint;
      new_ed25519_public_key: Uint8Array;
      new_signature: Uint8Array; // 64 bytes
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
    }
  | {
      type: 'multi_operation';
      senders: SenderInfo[];
      receivers: ReceiverInfo[];
      changers: ChangerInfo[];
      fee: bigint;
    }
  | {
      type: 'data';
      account: bigint;
      n_operation: bigint;
      fee: bigint;
      data: Uint8Array;
      senders: SenderInfo[];
      receivers: ReceiverInfo[];
      changers: ChangerInfo[];
    }
  | { type: 'validator_admin'; op: ValidatorAdminOp };

export type ValidatorAdminOp =
  | { tag: 'add'; ed25519_public_key: Uint8Array; activation_height: bigint }
  | { tag: 'remove'; validator_id: bigint; activation_height: bigint }
  | { tag: 'activate'; validator_id: bigint; activation_height: bigint }
  | { tag: 'deactivate'; validator_id: bigint; activation_height: bigint };

export interface Operation {
  chainId: bigint;
  payload: OperationPayload;
}

const OP_TAG = {
  transaction: 0x01,
  change_key: 0x02,
  recover_founds: 0x03,
  list_account_for_sale: 0x04,
  delist_account: 0x05,
  buy_account: 0x06,
  change_key_signed: 0x07,
  change_account_info: 0x08,
  multi_operation: 0x09,
  data: 0x0a,
  validator_admin: 0x0b,
} as const;

/** Create a transfer operation (one sender -> one receiver). */
export function createTransfer(
  chainId: bigint,
  sender: bigint,
  nOperation: bigint,
  to: bigint,
  amount: bigint,
  fee: bigint
): Operation {
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
    let offset = 0;
    for (const c of this.chunks) {
      out.set(c, offset);
      offset += c.length;
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
  w.optionalStr(c.new_name);
  w.u16(c.new_type);
  w.bytes(c.new_account_data);
  w.bytes(c.new_account_seal);
  w.u64(c.fee);
}

function writeSendersReceiversChangers(
  w: Writer,
  senders: SenderInfo[],
  receivers: ReceiverInfo[],
  changers: ChangerInfo[]
): void {
  w.u32(senders.length);
  for (const s of senders) writeSender(w, s);
  w.u32(receivers.length);
  for (const r of receivers) writeReceiver(w, r);
  w.u32(changers.length);
  for (const c of changers) writeChanger(w, c);
}

function writeValidatorAdmin(w: Writer, op: ValidatorAdminOp): void {
  switch (op.tag) {
    case 'add':
      w.u8(0);
      w.fixed(op.ed25519_public_key);
      w.u64(op.activation_height);
      break;
    case 'remove':
      w.u8(1);
      w.u64(op.validator_id);
      w.u64(op.activation_height);
      break;
    case 'activate':
      w.u8(2);
      w.u64(op.validator_id);
      w.u64(op.activation_height);
      break;
    case 'deactivate':
      w.u8(3);
      w.u64(op.validator_id);
      w.u64(op.activation_height);
      break;
  }
}

function writePayload(w: Writer, p: OperationPayload): void {
  switch (p.type) {
    case 'transaction':
      w.u8(OP_TAG.transaction);
      writeSendersReceiversChangers(w, p.senders, p.receivers, p.changers);
      w.u64(p.fee);
      break;
    case 'change_key':
      w.u8(OP_TAG.change_key);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      w.fixed(p.new_ed25519_public_key);
      break;
    case 'recover_founds':
      w.u8(OP_TAG.recover_founds);
      w.u64(p.account);
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
    case 'change_key_signed':
      w.u8(OP_TAG.change_key_signed);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      w.fixed(p.new_ed25519_public_key);
      w.fixed(p.new_signature);
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
    case 'multi_operation':
      w.u8(OP_TAG.multi_operation);
      writeSendersReceiversChangers(w, p.senders, p.receivers, p.changers);
      w.u64(p.fee);
      break;
    case 'data':
      w.u8(OP_TAG.data);
      w.u64(p.account);
      w.u64(p.n_operation);
      w.u64(p.fee);
      w.bytes(p.data);
      writeSendersReceiversChangers(w, p.senders, p.receivers, p.changers);
      break;
    case 'validator_admin':
      w.u8(OP_TAG.validator_admin);
      writeValidatorAdmin(w, p.op);
      break;
  }
}

/** Serialize payload + chain_id. This is the message that gets signed. */
export function serializeOperationStripped(op: Operation): Uint8Array {
  const w = new Writer();
  writePayload(w, op.payload);
  w.u64(op.chainId);
  return w.finish();
}

/**
 * Serialize a fully signed operation:
 *   stripped_bytes + u32(signature_count) + signatures
 */
export function serializeOperation(op: Operation, signatures: Uint8Array[]): Uint8Array {
  const w = new Writer();
  writePayload(w, op.payload);
  w.u64(op.chainId);
  w.u32(signatures.length);
  for (const sig of signatures) w.fixed(sig);
  return w.finish();
}

/** Convenience: sign the stripped bytes with a 64-byte Ed25519 signature. */
export function serializeSignedOperation(
  op: Operation,
  signature: Uint8Array
): Uint8Array {
  return serializeOperation(op, [signature]);
}
