import { formatAuge } from '../services/config';
import type { AccountInfo } from '../types';
import { QrCode } from './QrCode';
import { StatusBadge } from './StatusBadge';

const AugeIdIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
    <polygon points="12 2 20 7 20 17 12 22 4 17 4 7" />
    <circle cx="12" cy="12" r="3.5" />
  </svg>
);

/**
 * Camada 2 — Carteira Blockchain (dourado, ícone de AUGEID).
 * A single on-chain AUGEID owned by the user. Balance/name/state come from the
 * SafeBox via `getaccount`; nothing here is stored by the platform backend.
 */
export function BlockchainWalletCard({ account }: { account: AccountInfo }) {
  return (
    <div className="card blockchain-wallet-card" data-testid={`blockchain-wallet-card-${account.account_number}`}>
      <div className="wallet-card-head">
        <div>
          <div className="layer-chip layer-wallet">
            <AugeIdIcon />
            Carteira Blockchain
          </div>
          <div className="account-card-number">AUGEID #{account.account_number}</div>
          <div className="account-card-name">{account.name ?? 'Sem nome'}</div>
        </div>
        <StatusBadge state={account.state} />
      </div>

      <div className="account-card-balance">
        <span className="muted">Saldo</span>
        <strong data-testid={`wallet-balance-${account.account_number}`}>
          {formatAuge(account.balance)} AUGE
        </strong>
      </div>

      <div className="qr-container">
        <QrCode data={`augeid:${account.account_number}`} />
      </div>
    </div>
  );
}
