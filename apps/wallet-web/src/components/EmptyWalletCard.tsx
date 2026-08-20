import { Link } from 'react-router-dom';

/**
 * Carteira Blockchain ainda não ativada. The user has a platform account but
 * no AUGEID yet — only the two non-financial paths to obtain one are offered.
 */
export function EmptyWalletCard() {
  return (
    <div className="card empty-wallet-card" data-testid="empty-wallet-card">
      <div className="layer-chip layer-wallet">
        <span>Carteira Blockchain ainda não ativada</span>
      </div>
      <p className="muted">
        Você já possui uma Conta da Plataforma, mas ainda não recebeu um AUGEID.
        Enquanto isso, você pode gerenciar seu perfil normalmente.
      </p>
      <div className="actions">
        <Link to="/marketplace" className="btn btn-primary" data-testid="cta-buy-augeid">
          Comprar AUGEID
        </Link>
        <Link to="/gifts" className="btn" data-testid="cta-receive-gift">
          Receber Doação
        </Link>
      </div>
    </div>
  );
}
