// AUGECOIN Wallet Web — Wallet Registry (Camada 3 lógica).
//
// A bridge between web authentication and on-chain identity. It maps the logged
// platform user to their AUGEIDs (account numbers) and nothing else: no balance,
// no state, no private keys. Ownership/balance/transfer authority stay in the
// SafeBox; this registry only answers "which AUGEIDs belong to this user?".

import { apiFetch } from './api';
import type { LinkedWallet } from '../types';

export async function listLinkedWallets(): Promise<LinkedWallet[]> {
  const res = await apiFetch<{ wallets: LinkedWallet[] }>('/linked-wallets');
  return res.wallets;
}

/** Link an AUGEID to the current user (called after a successful buy/accept). */
export async function linkWallet(accountNumber: number): Promise<LinkedWallet> {
  const res = await apiFetch<{ wallet: LinkedWallet }>('/linked-wallets', {
    method: 'POST',
    body: { account_number: accountNumber },
    csrf: true,
  });
  return res.wallet;
}

export async function unlinkWallet(accountNumber: number): Promise<void> {
  await apiFetch(`/linked-wallets/${accountNumber}`, { method: 'DELETE', csrf: true });
}
