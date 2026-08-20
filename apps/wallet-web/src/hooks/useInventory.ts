import { useCallback, useMemo, useState } from 'react';
import { useSession } from '../app/AuthContext';
import { submitCancelSale, submitGift, submitSell, submitChangeKey } from '../services/operations';
import { CONFIG } from '../services/config';
import { isOwnedState, useLinkedWallets } from './useLinkedWallets';
import type { AccountInfo } from '../types';

interface InventoryState {
  reserved: AccountInfo[];
  forSale: AccountInfo[];
  owned: AccountInfo[];
  loading: boolean;
  busy: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  sell: (accountNumber: number, price: number) => Promise<boolean>;
  cancel: (accountNumber: number) => Promise<boolean>;
  gift: (accountNumber: number, recipientPublicKeyHex: string) => Promise<boolean>;
  transfer: (accountNumber: number, recipientPublicKeyHex: string) => Promise<boolean>;
}

/**
 * Inventory of the user's linked wallets, grouped by on-chain state. Reads
 * ownership from the wallet registry and state from `getaccount`.
 */
export function useInventory(): InventoryState {
  const session = useSession();
  const { wallets, loading, error, refresh } = useLinkedWallets();
  const [busy, setBusy] = useState(false);

  const accounts = useMemo(
    () => wallets.map((w) => w.account).filter((a): a is AccountInfo => a !== null),
    [wallets],
  );

  const reserved = useMemo(() => accounts.filter((a) => a.state === 'Reserved'), [accounts]);
  const forSale = useMemo(() => accounts.filter((a) => a.state === 'ForSale'), [accounts]);
  const owned = useMemo(() => accounts.filter((a) => isOwnedState(a.state)), [accounts]);

  const run = useCallback(
    async (fn: () => Promise<{ accepted: boolean }>) => {
      setBusy(true);
      try {
        const res = await fn();
        if (res.accepted) await refresh();
        return res.accepted;
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  const sell = useCallback(
    async (accountNumber: number, price: number) => {
      if (!session) return false;
      const acc = accounts.find((a) => a.account_number === accountNumber);
      if (!acc) return false;
      return run(() =>
        submitSell(session, {
          account: accountNumber,
          nOperation: acc.n_operation,
          salePrice: price,
          accountToPay: accountNumber,
          newPublicKeyHex: acc.account_key_ed_hex,
          lockedUntilBlock: 0,
          fee: CONFIG.MIN_FEE_AUGESAT,
        }),
      );
    },
    [session, accounts, run],
  );

  const cancel = useCallback(
    async (accountNumber: number) => {
      if (!session) return false;
      const acc = forSale.find((a) => a.account_number === accountNumber);
      if (!acc) return false;
      return run(() => submitCancelSale(session, { account: accountNumber, nOperation: acc.n_operation, fee: CONFIG.MIN_FEE_AUGESAT }));
    },
    [session, forSale, run],
  );

  const gift = useCallback(
    async (accountNumber: number, recipientPublicKeyHex: string) => {
      if (!session) return false;
      const acc = accounts.find((a) => a.account_number === accountNumber);
      if (!acc) return false;
      return run(() =>
        submitGift(session, {
          account: accountNumber,
          nOperation: acc.n_operation,
          recipientPublicKeyHex,
          fee: CONFIG.MIN_FEE_AUGESAT,
        }),
      );
    },
    [session, accounts, run],
  );

  const transfer = useCallback(
    async (accountNumber: number, recipientPublicKeyHex: string) => {
      if (!session) return false;
      const acc = accounts.find((a) => a.account_number === accountNumber);
      if (!acc) return false;
      return run(() =>
        submitChangeKey(session, {
          account: accountNumber,
          nOperation: acc.n_operation,
          fee: CONFIG.MIN_FEE_AUGESAT,
          newPublicKeyHex: recipientPublicKeyHex,
        }),
      );
    },
    [session, accounts, run],
  );

  return { reserved, forSale, owned, loading, busy, error, refresh, sell, cancel, gift, transfer };
}
