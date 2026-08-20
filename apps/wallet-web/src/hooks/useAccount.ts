import { useMemo } from 'react';
import { isOwnedState, useLinkedWallets } from './useLinkedWallets';
import type { AccountInfo } from '../types';

interface AccountStateShape {
  accounts: AccountInfo[];
  owned: AccountInfo[];
  hasAugeId: boolean;
  totalBalance: number;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Derived view over the wallet registry + on-chain `getaccount`. Kept as a thin
 * wrapper so Send/Receive/Marketplace/Dashboard can consume a single shape.
 */
export function useAccount(): AccountStateShape {
  const { wallets, hasWallet, loading, error, refresh } = useLinkedWallets();

  const accounts = useMemo(
    () => wallets.map((w) => w.account).filter((a): a is AccountInfo => a !== null),
    [wallets],
  );

  const owned = useMemo(
    () => accounts.filter((a) => isOwnedState(a.state)),
    [accounts],
  );

  const totalBalance = useMemo(() => owned.reduce((sum, a) => sum + a.balance, 0), [owned]);

  return { accounts, owned, hasAugeId: hasWallet, totalBalance, loading, error, refresh };
}
