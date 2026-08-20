import type { AccountState } from '../types';

const STATUS_META: Record<AccountState, { label: string; className: string }> = {
  Reserved: { label: 'Reserved', className: 'badge-reserved' },
  Owned: { label: 'Owned', className: 'badge-owned' },
  Normal: { label: 'Normal', className: 'badge-normal' },
  ForSale: { label: 'ForSale', className: 'badge-forsale' },
  GiftPending: { label: 'GiftPending', className: 'badge-giftpending' },
  Unknown: { label: 'Unknown', className: 'badge-unknown' },
  ForAtomicAccountSwap: { label: 'ForAtomicAccountSwap', className: 'badge-unknown' },
  ForAtomicCoinSwap: { label: 'ForAtomicCoinSwap', className: 'badge-unknown' },
};

export function StatusBadge({ state }: { state: AccountState | string }) {
  const meta = STATUS_META[state as AccountState] ?? { label: state, className: 'badge-unknown' };
  return (
    <span className={`status-badge ${meta.className}`} data-testid={`status-${meta.label}`}>
      {meta.label}
    </span>
  );
}
