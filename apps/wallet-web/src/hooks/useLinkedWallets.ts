import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAuth } from '../app/AuthContext';
import { CONFIG } from '../services/config';
import * as registry from '../services/walletRegistry';
import * as rpc from '../services/rpc';
import type { AccountInfo, LinkedWalletInfo } from '../types';

interface LinkedWalletsState {
  wallets: LinkedWalletInfo[];
  hasWallet: boolean;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  link: (accountNumber: number) => Promise<boolean>;
  unlink: (accountNumber: number) => Promise<boolean>;
}

/** On-chain states that imply the user actively controls/owns the AUGEID. */
export function isOwnedState(state: string | undefined): boolean {
  return state === 'Owned' || state === 'Normal';
}

async function enrich(accountNumber: number): Promise<AccountInfo | null> {
  return rpc.getAccount(accountNumber).catch(() => null);
}

/**
 * Wallet Registry hook. The source of truth for "which AUGEIDs belong to the
 * logged user" is the backend `linked_wallets` table; each AUGEID's balance,
 * name and state are then read from the blockchain via `getaccount`.
 */
export function useLinkedWallets(): LinkedWalletsState {
  const { user } = useAuth();
  const [wallets, setWallets] = useState<LinkedWalletInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!user) {
      setWallets([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const linked = await registry.listLinkedWallets();
      const enriched: LinkedWalletInfo[] = await Promise.all(
        linked.map(async (w) => ({ ...w, account: await enrich(w.account_number) })),
      );
      setWallets(enriched);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Falha ao carregar carteiras vinculadas.');
    } finally {
      setLoading(false);
    }
  }, [user]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), CONFIG.POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const link = useCallback(
    async (accountNumber: number) => {
      if (!user) return false;
      try {
        await registry.linkWallet(accountNumber);
        await refresh();
        return true;
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Falha ao vincular AUGEID.');
        return false;
      }
    },
    [user, refresh],
  );

  const unlink = useCallback(
    async (accountNumber: number) => {
      if (!user) return false;
      try {
        await registry.unlinkWallet(accountNumber);
        await refresh();
        return true;
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Falha ao desvincular AUGEID.');
        return false;
      }
    },
    [user, refresh],
  );

  const hasWallet = wallets.length > 0;

  return { wallets, hasWallet, loading, error, refresh, link, unlink };
}
