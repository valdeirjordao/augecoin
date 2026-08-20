import type { PlatformUser } from '../types';

const UserIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
    <circle cx="12" cy="8" r="4" />
    <path d="M4 21c0-4 3.6-6 8-6s8 2 8 6" />
  </svg>
);

/**
 * Camada 1 — Conta da Plataforma (azul, ícone de usuário).
 * Exists only in the wallet-web. No balance, no AUGEID.
 */
export function PlatformAccountCard({ user }: { user: PlatformUser }) {
  return (
    <div className="card platform-account-card" data-testid="platform-account-card">
      <div className="layer-chip layer-platform">
        <UserIcon />
        Conta da Plataforma
      </div>
      <div className="platform-account-name" data-testid="platform-display-name">
        {user.display_name}
      </div>
      <div className="muted mono" data-testid="platform-email">
        {user.email}
      </div>
      <div className="platform-account-meta">
        <span className="muted small">Identidade da plataforma · sem saldo</span>
      </div>
    </div>
  );
}
