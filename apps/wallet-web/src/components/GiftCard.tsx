import type { PendingGiftEntry } from '../types';

export function GiftCard({ gift, onAccept, busy }: { gift: PendingGiftEntry; onAccept: () => void; busy?: boolean }) {
  return (
    <div className="card gift-card" data-testid={`gift-card-${gift.account_number}`}>
      <div className="gift-card-head">
        <span className="account-card-number">AUGEID #{gift.account_number}</span>
        <span className="status-badge badge-giftpending">GiftPending</span>
      </div>
      <dl className="gift-card-fields">
        <div>
          <dt>Remetente</dt>
          <dd className="mono" title={gift.from_public_key_hex}>
            {shortHex(gift.from_public_key_hex)}
          </dd>
        </div>
        <div>
          <dt>Bloco</dt>
          <dd>#{gift.gifted_at_block}</dd>
        </div>
      </dl>
      {gift.name && <div className="muted">Nome atual: {gift.name}</div>}
      <button className="btn btn-primary" onClick={onAccept} disabled={busy} data-testid={`accept-${gift.account_number}`}>
        {busy ? 'Aceitando…' : 'Aceitar'}
      </button>
    </div>
  );
}

function shortHex(hex: string, len = 10): string {
  if (!hex) return '—';
  if (hex.length <= len * 2) return hex;
  return `${hex.slice(0, len)}…${hex.slice(-4)}`;
}
