import { NavLink, Outlet } from 'react-router-dom';
import { useAuth } from '../app/AuthContext';
import { useLinkedWallets } from '../hooks/useLinkedWallets';

interface NavItem {
  to: string;
  label: string;
  end?: boolean;
}

const ALWAYS: NavItem[] = [
  { to: '/', label: 'Dashboard', end: true },
  { to: '/marketplace', label: 'Marketplace' },
  { to: '/gifts', label: 'Receber Doação' },
  { to: '/validator', label: 'Validador' },
  { to: '/profile', label: 'Perfil' },
];

// Financial items only appear once the user has at least one AUGEID.
const REQUIRES_AUGEID: NavItem[] = [
  { to: '/send', label: 'Enviar' },
  { to: '/receive', label: 'Receber' },
  { to: '/history', label: 'Histórico' },
  { to: '/my-wallets', label: 'Minhas Carteiras' },
];

export function Layout() {
  const { user, logout } = useAuth();
  const { hasWallet } = useLinkedWallets();

  return (
    <div className="app-shell">
      <header className="topbar">
        <span className="brand">
          <img src="/logo.svg" className="brand-logo" alt="" />
          <span className="brand-text">
            AUGECOIN<span className="brand-sub">Wallet</span>
          </span>
        </span>
        <div className="topbar-actions">
          {user && (
            <span className="muted" data-testid="topbar-user">
              {user.display_name}
            </span>
          )}
          <button className="btn btn-ghost" onClick={() => void logout()} data-testid="logout-btn">
            Sair
          </button>
        </div>
      </header>

      <nav className="sidebar" aria-label="Navegação">
        <div className="nav-section-label layer-chip layer-platform">Conta da Plataforma</div>
        {ALWAYS.map((n) => (
          <NavLink
            key={n.to}
            to={n.to}
            end={n.end}
            className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}
          >
            {n.label}
          </NavLink>
        ))}

        <div className="nav-section-label layer-chip layer-wallet">Carteira Blockchain</div>
        {hasWallet ? (
          REQUIRES_AUGEID.map((n) => (
            <NavLink
              key={n.to}
              to={n.to}
              end={n.end}
              className={({ isActive }) => (isActive ? 'nav-link active' : 'nav-link')}
              data-testid={`nav-${n.to.replace('/', '')}`}
            >
              {n.label}
            </NavLink>
          ))
        ) : (
          <p className="nav-locked muted small" data-testid="nav-locked">
            Ative um AUGEID para desbloquear operações financeiras.
          </p>
        )}
      </nav>

      <main className="main">
        <Outlet />
      </main>
    </div>
  );
}
