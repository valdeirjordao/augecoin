import { Link } from 'react-router-dom';
import { useAuth } from './AuthContext';

export function Validator() {
  const { user } = useAuth();
  if (!user) return null;

  return (
    <div className="page">
      <div className="page-head">
        <h2>Seja um Validador</h2>
        <p className="muted">
          Execute um nó, produza blocos e receba recompensas em AUGE e AUGEIDs.
        </p>
      </div>

      <div className="card">
        <h3 className="section-title">Como funciona</h3>
        <ol className="how-it-works">
          <li>Escolha um plano e compre uma licença.</li>
          <li>Baixe e instale o software Validador (Windows ou Linux).</li>
          <li>Ative com sua licença — o software gera a chave Ed25519 localmente.</li>
          <li>O servidor valida a licença e registra a chave pública.</li>
          <li>A blockchain autoriza o validador e ele começa a produzir blocos.</li>
          <li>Acompanhe os ganhos em tempo real.</li>
        </ol>
      </div>

      <div className="grid-stats">
        <div className="stat-card">
          <div className="stat-label">Recompensa por bloco</div>
          <div className="stat-value">7.25 AUGE</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">AUGEIDs por bloco</div>
          <div className="stat-value">10</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Tempo online</div>
          <div className="stat-value">24/7</div>
        </div>
      </div>

      <div className="card">
        <h3 className="section-title">Requisitos</h3>
        <ul className="requirements">
          <li>Sistema Windows ou Linux (64-bit).</li>
          <li>Conexão estável com a internet.</li>
          <li>Mínimo 4 GB de RAM e 20 GB de disco.</li>
          <li>A chave privada fica sempre na sua máquina — nunca sai dela.</li>
        </ul>
      </div>

      <div className="actions">
        <Link to="/validator/plans" className="btn btn-primary" data-testid="be-validator-btn">
          Quero ser Validador
        </Link>
      </div>
    </div>
  );
}
