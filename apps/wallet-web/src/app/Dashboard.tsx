import { Link } from 'react-router-dom';
import { useAuth } from './AuthContext';
import { useAccount } from '../hooks/useAccount';
import { PlatformAccountCard } from '../components/PlatformAccountCard';
import { BlockchainWalletCard } from '../components/BlockchainWalletCard';
import { EmptyWalletCard } from '../components/EmptyWalletCard';
import { Skeleton } from '../components/Modal';
import { formatAuge } from '../services/config';

export function Dashboard() {
  const { user } = useAuth();
  const { accounts, owned, hasAugeId, totalBalance, loading } = useAccount();

  if (!user) return null;

  return (
    <div className="page">
      <div className="page-head">
        <h2>Dashboard</h2>
        <p className="muted">
          Conta da Plataforma e Carteira Blockchain são coisas separadas.
        </p>
      </div>

      <PlatformAccountCard user={user} />

      {loading && accounts.length === 0 ? (
        <Skeleton rows={3} />
      ) : !hasAugeId ? (
        <EmptyWalletCard />
      ) : (
        <>
          <div className="grid-stats">
            <div className="stat-card">
              <div className="stat-label">Saldo total</div>
              <div className="stat-value" data-testid="dashboard-balance">
                {formatAuge(totalBalance)} AUGE
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-label">Carteiras vinculadas</div>
              <div className="stat-value">{accounts.length}</div>
            </div>
          </div>

          <div className="section">
            <h3 className="section-title">Suas carteiras blockchain</h3>
            <div className="account-list">
              {accounts.map((a) => (
                <BlockchainWalletCard key={a.account_number} account={a} />
              ))}
            </div>
          </div>

          <div className="actions">
            <Link to="/send" className="btn btn-primary" data-testid="send-btn-link">
              Enviar AUGE
            </Link>
            <Link to="/receive" className="btn">
              Receber AUGE
            </Link>
          </div>
        </>
      )}
    </div>
  );
}
