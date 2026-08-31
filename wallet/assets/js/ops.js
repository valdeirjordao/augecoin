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
  createContractOp,
  serializeOperationStripped, serializeSignedOperation,
} from './tx.js';
import { encodeCreateContractData, encodeTokenCall, encodeExecuteContractData } from './contracts.js?v=20260831-1';
import { sendOperation, sellAccount, cancelSale, buyAccount, giftAccount, acceptGift, getAccountByNumber } from './rpc.js';

async function signStripped(op, session) {
  const stripped = serializeOperationStripped(op);
  return signMessageHex(session.mnemonic, CONFIG.DERIVATION_INDEX, bytesToHex(stripped));
}

/** Busca o `n_operation` atualizado do nó para a conta, evitando o erro
 *  "invalid n_operation" causado por valor em cache obsoleto na carteira.
 *  Usa `fallback` se a consulta falhar. */
async function freshNOperation(accountNumber, fallback) {
  try {
    const acc = await getAccountByNumber(Number(accountNumber));
    if (acc && typeof acc.n_operation === 'number') return acc.n_operation;
  } catch { /* mantém fallback */ }
  return fallback;
}

export async function submitTransfer(session, args) {
  const nOperation = await freshNOperation(args.sender, args.nOperation);
  const op = createTransfer(session.chainId, args.sender, nOperation, args.to, args.amount, args.fee);
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

export async function submitSell(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const op = createListAccountForSale(session.chainId, args.account, nOperation, args.salePrice, args.accountToPay, hexToBytes(args.newPublicKeyHex), args.lockedUntilBlock, args.fee);
  const sig = await signStripped(op, session);
  return sellAccount({
    account: args.account, n_operation: nOperation, sale_price: args.salePrice,
    account_to_pay: args.accountToPay, locked_until_block: args.lockedUntilBlock, fee: args.fee,
    new_public_key_hex: args.newPublicKeyHex, signature_hex: sig,
  });
}

export async function submitCancelSale(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const op = createDelistAccount(session.chainId, args.account, nOperation, args.fee);
  const sig = await signStripped(op, session);
  return cancelSale({ account: args.account, n_operation: nOperation, fee: args.fee, signature_hex: sig });
}

export async function submitBuy(session, args) {
  const nOperation = await freshNOperation(args.buyerAccount, args.nOperation);
  const op = createBuyAccount(session.chainId, args.buyerAccount, nOperation, args.accountToPurchase, args.amount, args.fee, hexToBytes(args.newPublicKeyHex), args.sellerAccount);
  const sig = await signStripped(op, session);
  return buyAccount({
    buyer_account: args.buyerAccount, n_operation: nOperation, account_to_purchase: args.accountToPurchase,
    amount: args.amount, fee: args.fee, new_public_key_hex: args.newPublicKeyHex, seller_account: args.sellerAccount, signature_hex: sig,
  });
}

export async function submitGift(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const op = createGiftAccount(session.chainId, args.account, nOperation, hexToBytes(args.recipientPublicKeyHex), args.fee);
  const sig = await signStripped(op, session);
  return giftAccount({ account: args.account, n_operation: nOperation, recipient_public_key_hex: args.recipientPublicKeyHex, fee: args.fee, signature_hex: sig });
}

export async function submitAcceptGift(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const op = createAcceptGift(session.chainId, args.account, nOperation, args.fee);
  const sig = await signStripped(op, session);
  return acceptGift({ account: args.account, n_operation: nOperation, fee: args.fee, signature_hex: sig });
}

export async function submitChangeKey(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const op = createChangeKey(session.chainId, args.account, nOperation, args.fee, hexToBytes(args.newPublicKeyHex));
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

export async function submitChangeAccountInfo(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const op = createChangeAccountInfo(session.chainId, args.account, nOperation, args.fee, hexToBytes(args.newPublicKeyHex), args.newName, args.newType, hexToBytes(args.newAccountDataHex || ''), hexToBytes(args.newAccountSealHex || ''));
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

// ── Smart contracts (assinados pelo usuário, sem admin key) ───────────

/**
 * Executa um contrato com a chave do próprio membro (op Contract 0x10).
 * Para AUGE20 transfer: callData = encodeTokenCall({kind:'transfer', to, amount});
 * o contrato debita o saldo de tokens do assinante.
 */
export async function submitContractExecute(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const data = encodeExecuteContractData(args.contractIdHex, args.callData);
  const op = createContractOp(session.chainId, args.account, nOperation, args.fee, 2, data);
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

/** Instancia AUGE20 com CreateContract, assinado pela carteira do proprietário. */
export async function submitContractCreate(session, args) {
  const nOperation = await freshNOperation(args.account, args.nOperation);
  const data = encodeCreateContractData(args.codeId, args.instantiateData);
  const op = createContractOp(session.chainId, args.account, nOperation, args.fee, 1, data);
  const sig = await signStripped(op, session);
  return sendOperation(bytesToHex(serializeSignedOperation(op, [hexToBytes(sig)])));
}

/** Transferência de token AUGE20 assinada pelo membro. */
export async function submitTokenTransfer(session, args) {
  const callData = encodeTokenCall({ kind: 'transfer', to: args.to, amount: args.amount });
  return submitContractExecute(session, {
    account: args.account, nOperation: args.nOperation,
    contractIdHex: args.contractIdHex, callData, fee: args.fee,
  });
}

export async function submitTokenMint(session, args) {
  return submitContractExecute(session, {
    ...args,
    callData: encodeTokenCall({ kind: 'mint', to: args.to, amount: args.amount }),
  });
}

export async function submitTokenBurn(session, args) {
  return submitContractExecute(session, {
    ...args,
    callData: encodeTokenCall({ kind: 'burn', amount: args.amount }),
  });
}
