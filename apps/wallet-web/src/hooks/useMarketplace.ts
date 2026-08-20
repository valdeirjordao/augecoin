import { useCallback, useEffect, useMemo, useState } from 'react';
import { useSession } from '../app/AuthContext';
import { CONFIG } from '../services/config';
import { submitBuy } from '../services/operations';
import * as platform from '../services/platform';
import * as rpc from '../services/rpc';
import { linkWallet } from '../services/walletRegistry';
import type { MarketplaceEntry } from '../types';
import { useAccount } from './useAccount';

export interface MarketplaceItem extends MarketplaceEntry {
  name: string | null;
  account_to_pay: number;
  sellerName: string;
}

export type MarketplaceSort = 'price-asc' | 'price-desc' | 'recent' | 'number';

interface MarketplaceState {
  items: MarketplaceItem[];
  loading: boolean;
  error: string | null;
  sort: MarketplaceSort;
  search: string;
  setSort: (s: MarketplaceSort) => void;
  setSearch: (s: string) => void;
  filtered: MarketplaceItem[];
  refresh: () => Promise<void>;
  buy: (item: MarketplaceItem) => Promise<boolean>;
  buying: boolean;
}

export function useMarketplace(): MarketplaceState {
  const session = useSession();
  const account = useAccount();
  const [items, setItems] = useState<MarketplaceItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [buying, setBuying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sort, setSort] = useState<MarketplaceSort>('number');
  const [search, setSearch] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await rpc.listAccountsForSale();
      const enriched = await Promise.all(
        res.entries.map(async (entry) => {
          const info = await rpc.getAccount(entry.account_number).catch(() => null);
          const sellerName = (await platform.displayNameByKey(entry.seller_public_key_hex)) ?? 'Validador';
          return {
            ...entry,
            name: info?.name ?? null,
            account_to_pay: info?.account_to_pay ?? 0,
            sellerName,
          };
        }),
      );
      setItems(enriched);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Falha ao carregar marketplace.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), CONFIG.POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const filtered = useMemo(() => {
    const term = search.trim().toLowerCase();
    let list = items.filter((i) => {
      if (!term) return true;
      const num = String(i.account_number);
      const name = (i.name ?? '').toLowerCase();
      return num.includes(term) || name.includes(term);
    });
    list = [...list].sort((a, b) => {
      switch (sort) {
        case 'price-asc':
          return a.price - b.price;
        case 'price-desc':
          return b.price - a.price;
        case 'recent':
          return b.listed_at_block - a.listed_at_block;
        case 'number':
        default:
          return a.account_number - b.account_number;
      }
    });
    return list;
  }, [items, sort, search]);

  const buy = useCallback(
    async (item: MarketplaceItem) => {
      if (!session) return false;
      const buyer = account.owned[0];
      if (!buyer) {
        setError('Você precisa de um AUGEID com saldo para financiar a compra.');
        return false;
      }
      setBuying(true);
      try {
        const res = await submitBuy(session, {
          buyerAccount: buyer.account_number,
          nOperation: buyer.n_operation,
          accountToPurchase: item.account_number,
          amount: item.price,
          fee: CONFIG.MIN_FEE_AUGESAT,
          newPublicKeyHex: session.publicKeyHex,
          sellerAccount: item.account_to_pay,
        });
        if (res.accepted) {
          // Vinculação automática: buyAccount retornou sucesso -> linked_wallets.
          await linkWallet(item.account_number);
          await refresh();
          await account.refresh();
          return true;
        }
        setError(res.error ?? 'Compra rejeitada.');
        return false;
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Falha ao comprar.');
        return false;
      } finally {
        setBuying(false);
      }
    },
    [session, account, refresh],
  );

  return {
    items,
    loading,
    error,
    sort,
    search,
    setSort,
    setSearch,
    filtered,
    refresh,
    buy,
    buying,
  };
}
