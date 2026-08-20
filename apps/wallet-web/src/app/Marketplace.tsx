import { useState } from 'react';
import { useMarketplace } from '../hooks/useMarketplace';
import type { MarketplaceItem } from '../hooks/useMarketplace';
import { MarketplaceCard } from '../components/MarketplaceCard';
import { Modal, Skeleton } from '../components/Modal';
import { useToast } from './ToastContext';
import { formatAuge } from '../services/config';

export function Marketplace() {
  const { filtered, loading, error, sort, setSort, search, setSearch, buy, buying } = useMarketplace();
  const { toast } = useToast();
  const [pending, setPending] = useState<MarketplaceItem | null>(null);

  const confirmBuy = async () => {
    if (!pending) return;
    const ok = await buy(pending);
    if (ok) {
      toast(`AUGEID #${pending.account_number} comprado!`, 'success');
      setPending(null);
    } else {
      toast('Falha ao comprar AUGEID.', 'error');
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Marketplace</h2>
        <p className="muted">AUGEIDs disponíveis para compra.</p>
      </div>

      <div className="toolbar">
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Buscar por nome ou número"
          className="text-input"
          data-testid="marketplace-search"
        />
        <select value={sort} onChange={(e) => setSort(e.target.value as never)} className="text-input" data-testid="marketplace-sort">
          <option value="number">Número do AUGEID</option>
          <option value="price-asc">Menor preço</option>
          <option value="price-desc">Maior preço</option>
          <option value="recent">Mais recente</option>
        </select>
      </div>

      {error && <p className="error">{error}</p>}

      {loading && filtered.length === 0 ? (
        <Skeleton rows={4} />
      ) : filtered.length === 0 ? (
        <div className="empty-state">
          <p className="muted">Nenhum AUGEID à venda.</p>
        </div>
      ) : (
        <div className="card-grid" data-testid="marketplace-list">
          {filtered.map((item) => (
            <MarketplaceCard key={item.account_number} item={item} onBuy={() => setPending(item)} busy={buying} />
          ))}
        </div>
      )}

      <Modal open={!!pending} title="Confirmar compra" onClose={() => setPending(null)}>
        {pending && (
          <div className="confirm-box" data-testid="buy-confirm">
            <p>
              Você está comprando o <strong>AUGEID #{pending.account_number}</strong>
              {pending.name ? ` (${pending.name})` : ''}.
            </p>
            <p>
              Preço: <strong className="price">{formatAuge(pending.price)} AUGE</strong>
            </p>
            <p className="muted">Vendedor: {pending.sellerName}</p>
            <div className="actions">
              <button className="btn btn-primary" onClick={confirmBuy} disabled={buying} data-testid="confirm-buy">
                Confirmar compra
              </button>
              <button className="btn" onClick={() => setPending(null)}>
                Cancelar
              </button>
            </div>
          </div>
        )}
      </Modal>
    </div>
  );
}
