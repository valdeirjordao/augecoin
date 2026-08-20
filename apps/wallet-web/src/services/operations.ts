// AUGECOIN Wallet Web — high-level operation submission (build + sign + submit).
//
// Every on-chain action flows through here: the operation is built, its stripped
// bytes signed with the session key, and the signed result submitted to the node.
// No consensus logic lives in the UI — the node remains the final validator.

import { CONFIG } from './config';
import { bytesToHex, hexToBytes } from './transactions';
import {
  createAcceptGift,
  createBuyAccount,
  createChangeAccountInfo,
  createChangeKey,
  createDelistAccount,
  createGiftAccount,
  createListAccountForSale,
  createTransfer,
  serializeOperationStripped,
  serializeSignedOperation,
} from './transactions';
import type { Operation } from './transactions';
import { signMessageHex } from './crypto';
import * as rpc from './rpc';
import type { LifecycleOpResult } from '../types';

export interface Session {
  mnemonic: string;
  publicKeyHex: string;
  chainId: bigint;
}

async function signStripped(op: Operation, session: Session): Promise<string> {
  const stripped = serializeOperationStripped(op);
  return signMessageHex(session.mnemonic, CONFIG.DERIVATION_INDEX, bytesToHex(stripped));
}

export async function submitTransfer(
  session: Session,
  args: { sender: number; nOperation: number; to: number; amount: number; fee: number },
): Promise<{ accepted: boolean; op_hash_hex: string; error: string | null }> {
  const op = createTransfer(session.chainId, args.sender, args.nOperation, args.to, args.amount, args.fee);
  const sigHex = await signStripped(op, session);
  const full = serializeSignedOperation(op, [hexToBytes(sigHex)]);
  return rpc.sendOperation(bytesToHex(full));
}

export async function submitSell(
  session: Session,
  args: {
    account: number;
    nOperation: number;
    salePrice: number;
    accountToPay: number;
    newPublicKeyHex: string;
    lockedUntilBlock: number;
    fee: number;
  },
): Promise<LifecycleOpResult> {
  const op = createListAccountForSale(
    session.chainId,
    args.account,
    args.nOperation,
    args.salePrice,
    args.accountToPay,
    hexToBytes(args.newPublicKeyHex),
    args.lockedUntilBlock,
    args.fee,
  );
  const signatureHex = await signStripped(op, session);
  return rpc.sellAccount({
    account: args.account,
    n_operation: args.nOperation,
    sale_price: args.salePrice,
    account_to_pay: args.accountToPay,
    locked_until_block: args.lockedUntilBlock,
    fee: args.fee,
    new_public_key_hex: args.newPublicKeyHex,
    signature_hex: signatureHex,
  });
}

export async function submitCancelSale(
  session: Session,
  args: { account: number; nOperation: number; fee: number },
): Promise<LifecycleOpResult> {
  const op = createDelistAccount(session.chainId, args.account, args.nOperation, args.fee);
  const signatureHex = await signStripped(op, session);
  return rpc.cancelSale({
    account: args.account,
    n_operation: args.nOperation,
    fee: args.fee,
    signature_hex: signatureHex,
  });
}

export async function submitBuy(
  session: Session,
  args: {
    buyerAccount: number;
    nOperation: number;
    accountToPurchase: number;
    amount: number;
    fee: number;
    newPublicKeyHex: string;
    sellerAccount: number;
  },
): Promise<LifecycleOpResult> {
  const op = createBuyAccount(
    session.chainId,
    args.buyerAccount,
    args.nOperation,
    args.accountToPurchase,
    args.amount,
    args.fee,
    hexToBytes(args.newPublicKeyHex),
    args.sellerAccount,
  );
  const signatureHex = await signStripped(op, session);
  return rpc.buyAccount({
    buyer_account: args.buyerAccount,
    n_operation: args.nOperation,
    account_to_purchase: args.accountToPurchase,
    amount: args.amount,
    fee: args.fee,
    new_public_key_hex: args.newPublicKeyHex,
    seller_account: args.sellerAccount,
    signature_hex: signatureHex,
  });
}

export async function submitGift(
  session: Session,
  args: { account: number; nOperation: number; recipientPublicKeyHex: string; fee: number },
): Promise<LifecycleOpResult> {
  const op = createGiftAccount(
    session.chainId,
    args.account,
    args.nOperation,
    hexToBytes(args.recipientPublicKeyHex),
    args.fee,
  );
  const signatureHex = await signStripped(op, session);
  return rpc.giftAccount({
    account: args.account,
    n_operation: args.nOperation,
    recipient_public_key_hex: args.recipientPublicKeyHex,
    fee: args.fee,
    signature_hex: signatureHex,
  });
}

export async function submitAcceptGift(
  session: Session,
  args: { account: number; nOperation: number; fee: number },
): Promise<LifecycleOpResult> {
  const op = createAcceptGift(session.chainId, args.account, args.nOperation, args.fee);
  const signatureHex = await signStripped(op, session);
  return rpc.acceptGift({
    account: args.account,
    n_operation: args.nOperation,
    fee: args.fee,
    signature_hex: signatureHex,
  });
}

export async function submitChangeKey(
  session: Session,
  args: { account: number; nOperation: number; fee: number; newPublicKeyHex: string },
): Promise<{ accepted: boolean; op_hash_hex: string; error: string | null }> {
  const op = createChangeKey(
    session.chainId,
    args.account,
    args.nOperation,
    args.fee,
    hexToBytes(args.newPublicKeyHex),
  );
  const signatureHex = await signStripped(op, session);
  const full = serializeSignedOperation(op, [hexToBytes(signatureHex)]);
  return rpc.sendOperation(bytesToHex(full));
}

export async function submitChangeAccountInfo(
  session: Session,
  args: {
    account: number;
    nOperation: number;
    fee: number;
    newPublicKeyHex: string;
    newName: string | null;
    newType: number;
    newAccountDataHex: string;
    newAccountSealHex: string;
  },
): Promise<{ accepted: boolean; op_hash_hex: string; error: string | null }> {
  const op = createChangeAccountInfo(
    session.chainId,
    args.account,
    args.nOperation,
    args.fee,
    hexToBytes(args.newPublicKeyHex),
    args.newName,
    args.newType,
    hexToBytes(args.newAccountDataHex || ''),
    hexToBytes(args.newAccountSealHex || ''),
  );
  const signatureHex = await signStripped(op, session);
  const full = serializeSignedOperation(op, [hexToBytes(signatureHex)]);
  return rpc.sendOperation(bytesToHex(full));
}
