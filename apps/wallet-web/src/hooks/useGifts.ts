import { useCallback, useEffect, useState } from 'react';
import { useSession } from '../app/AuthContext';
import { CONFIG } from '../services/config';
import { submitAcceptGift } from '../services/operations';
import * as rpc from '../services/rpc';
import { linkWallet } from '../services/walletRegistry';
import type { PendingGiftEntry } from '../types';

interface GiftsState {
  gifts: PendingGiftEntry[];
  loading: boolean;
  busy: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  accept: (accountNumber: number) => Promise<boolean>;
}

export function useGifts(): GiftsState {
  const session = useSession();
  const [gifts, setGifts] = useState<PendingGiftEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!session) {
      setGifts([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await rpc.listPendingGifts(session.publicKeyHex);
      setGifts(res.entries);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Falha ao carregar presentes.');
    } finally {
      setLoading(false);
    }
  }, [session]);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), CONFIG.POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const accept = useCallback(
    async (accountNumber: number) => {
      if (!session) return false;
      setBusy(true);
      setError(null);
      try {
        const acc = await rpc.getAccount(accountNumber);
        const res = await submitAcceptGift(session, {
          account: accountNumber,
          nOperation: acc.n_operation,
          fee: CONFIG.MIN_FEE_AUGESAT,
        });
        if (res.accepted) {
          // Vinculação automática: acceptGift retornou sucesso -> linked_wallets.
          await linkWallet(accountNumber);
          await refresh();
          return true;
        }
        setError(res.error ?? 'Aceite rejeitado.');
        return false;
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Falha ao aceitar.');
        return false;
      } finally {
        setBusy(false);
      }
    },
    [session, refresh],
  );

  return { gifts, loading, busy, error, refresh, accept };
}
