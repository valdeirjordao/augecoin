// AUGECOIN Wallet — high-level operation submission (build + sign + submit).
//
// Every on-chain action flows through here. No consensus logic lives in the UI;
// the node remains the final validator.

import { CONFIG } from './config.js';
import { bytesToHex, hexToBytes } from './utils.js';
import { signMessageHex } from './crypto.js';
import {
  createTransfer, createListAccountForSale, createDelistAccount, createBuyAccount,
  createChangeKey, createChangeAccountInfo, createGiftAccount, createAcceptGift,
  serializeOperationStripped, serializeSignedOperation,
} from './tx.js';
import { sendOperation, sellAccount, cancelSale, buyAccount, giftAccount, acceptGift } from './rpc.js';

async function signStripped(op, session) {
  const stripped = serializeOperationStripped(op);
  return signMessageHex(session.mnemonic, CONFIG.DERIVATION_INDEX, bytesToHex(stripped));
}

export async function submitTransfer(session, args) {
  const op = createTransfer(session.chainId, args.sender, args.nOperation, args.to, args.amount, args.fee);
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

export async function submitSell(session, args) {
  const op = createListAccountForSale(session.chainId, args.account, args.nOperation, args.salePrice, args.accountToPay, hexToBytes(args.newPublicKeyHex), args.lockedUntilBlock, args.fee);
  const sig = await signStripped(op, session);
  return sellAccount({
    account: args.account, n_operation: args.nOperation, sale_price: args.salePrice,
    account_to_pay: args.accountToPay, locked_until_block: args.lockedUntilBlock, fee: args.fee,
    new_public_key_hex: args.newPublicKeyHex, signature_hex: sig,
  });
}

export async function submitCancelSale(session, args) {
  const op = createDelistAccount(session.chainId, args.account, args.nOperation, args.fee);
  const sig = await signStripped(op, session);
  return cancelSale({ account: args.account, n_operation: args.nOperation, fee: args.fee, signature_hex: sig });
}

export async function submitBuy(session, args) {
  const op = createBuyAccount(session.chainId, args.buyerAccount, args.nOperation, args.accountToPurchase, args.amount, args.fee, hexToBytes(args.newPublicKeyHex), args.sellerAccount);
  const sig = await signStripped(op, session);
  return buyAccount({
    buyer_account: args.buyerAccount, n_operation: args.nOperation, account_to_purchase: args.accountToPurchase,
    amount: args.amount, fee: args.fee, new_public_key_hex: args.newPublicKeyHex, seller_account: args.sellerAccount, signature_hex: sig,
  });
}

export async function submitGift(session, args) {
  const op = createGiftAccount(session.chainId, args.account, args.nOperation, hexToBytes(args.recipientPublicKeyHex), args.fee);
  const sig = await signStripped(op, session);
  return giftAccount({ account: args.account, n_operation: args.nOperation, recipient_public_key_hex: args.recipientPublicKeyHex, fee: args.fee, signature_hex: sig });
}

export async function submitAcceptGift(session, args) {
  const op = createAcceptGift(session.chainId, args.account, args.nOperation, args.fee);
  const sig = await signStripped(op, session);
  return acceptGift({ account: args.account, n_operation: args.nOperation, fee: args.fee, signature_hex: sig });
}

export async function submitChangeKey(session, args) {
  const op = createChangeKey(session.chainId, args.account, args.nOperation, args.fee, hexToBytes(args.newPublicKeyHex));
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

export async function submitChangeAccountInfo(session, args) {
  const op = createChangeAccountInfo(session.chainId, args.account, args.nOperation, args.fee, hexToBytes(args.newPublicKeyHex), args.newName, args.newType, hexToBytes(args.newAccountDataHex || ''), hexToBytes(args.newAccountSealHex || ''));
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}
