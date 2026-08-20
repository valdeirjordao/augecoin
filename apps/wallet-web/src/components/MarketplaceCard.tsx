import { formatAuge } from '../services/config';
import type { MarketplaceItem } from '../hooks/useMarketplace';

export function MarketplaceCard({ item, onBuy, busy }: { item: MarketplaceItem; onBuy: () => void; busy?: boolean }) {
  return (
    <div className="card marketplace-card" data-testid={`marketplace-card-${item.account_number}`}>
      <div className="marketplace-card-head">
        <span className="account-card-number">AUGEID #{item.account_number}</span>
        <span className="badge badge-forsale status-badge">Disponível</span>
      </div>
      <div className="marketplace-card-name">{item.name ?? 'Sem nome'}</div>
      <dl className="marketplace-card-fields">
        <div>
          <dt>Preço</dt>
          <dd className="price">{formatAuge(item.price)} AUGE</dd>
        </div>
        <div>
          <dt>Vendedor</dt>
          <dd>{item.sellerName}</dd>
        </div>
      </dl>
      <button className="btn btn-primary" onClick={onBuy} disabled={busy} data-testid={`buy-${item.account_number}`}>
        {busy ? 'Comprando…' : 'Comprar'}
      </button>
    </div>
  );
}
