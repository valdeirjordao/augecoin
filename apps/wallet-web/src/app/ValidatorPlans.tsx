import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from './AuthContext';
import { listPlans, createOrder, listOrders } from '../services/validator';
import type { Plan, OrderMethod, ValidatorOrder } from '../types';

const METHOD_LABELS: Record<OrderMethod, string> = {
  pix: 'PIX',
  usdt: 'USDT',
  auge: 'AUGE',
};

const PLAN_BENEFITS: Record<string, string[]> = {
  monthly: ['Validação de blocos', 'Ganhos em AUGE e AUGEIDs', 'Suporte por e-mail'],
  semiannual: ['Tudo do plano mensal', '2 meses grátis', 'Prioridade no suporte'],
  annual: ['Tudo do plano semestral', '4 meses grátis', 'Suporte prioritário 24/7'],
};

export function ValidatorPlans() {
  const { user } = useAuth();
  const [plans, setPlans] = useState<Plan[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedPlan, setSelectedPlan] = useState<Plan | null>(null);
  const [method, setMethod] = useState<OrderMethod>('pix');
  const [creating, setCreating] = useState(false);
  const [order, setOrder] = useState<ValidatorOrder | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listPlans()
      .then(({ plans }) => setPlans(plans))
      .catch(() => setError('Não foi possível carregar os planos.'))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    listOrders().catch(() => {});
  }, []);

  if (!user) return null;

  async function buy(plan: Plan) {
    setError(null);
    setCreating(true);
    try {
      const { order } = await createOrder({ plan: plan.id, method });
      setOrder(order);
      setSelectedPlan(plan);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Erro ao criar pedido.');
    } finally {
      setCreating(false);
    }
  }

  function reset() {
    setOrder(null);
    setSelectedPlan(null);
  }

  if (order && selectedPlan) {
    return (
      <div className="page">
        <div className="page-head">
          <h2>Pedido de licença</h2>
          <p className="muted">Aguardando confirmação do pagamento.</p>
        </div>
        <div className="card">
          <h3 className="section-title">Resumo</h3>
          <div className="kv">
            <span className="kv-label">Plano</span>
            <span className="kv-value">{selectedPlan.label} — US${selectedPlan.usd}</span>
          </div>
          <div className="kv">
            <span className="kv-label">Método</span>
            <span className="kv-value">{METHOD_LABELS[method]}</span>
          </div>
          <div className="kv">
            <span className="kv-label">Referência</span>
            <span className="kv-value mono" data-testid="order-id">{order.id}</span>
          </div>
          <div className="kv">
            <span className="kv-label">Status</span>
            <span className="kv-value">
              <span className="status-badge badge-forsale">Aguardando pagamento</span>
            </span>
          </div>
        </div>
        <div className="card">
          <h3 className="section-title">Como pagar ({METHOD_LABELS[method]})</h3>
          <p className="muted">
            Realize o pagamento usando a referência acima. Assim que o pagamento for
            confirmado, você poderá emitir sua licença na página{' '}
            <Link to="/validator/license">Minha Licença</Link>.
          </p>
        </div>
        <div className="actions">
          <Link to="/validator/license" className="btn btn-primary">
            Ver minha licença
          </Link>
          <button className="btn" onClick={reset}>
            Novo pedido
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-head">
        <h2>Planos</h2>
        <p className="muted">Escolha o plano de validação e comece a produzir blocos.</p>
      </div>

      {error && <p className="login-error">{error}</p>}

      {loading ? (
        <p className="muted">Carregando planos…</p>
      ) : (
        <div className="card-grid">
          {plans.map((plan) => (
            <div className="card plan-card" key={plan.id}>
              <h3 className="plan-name">{plan.label}</h3>
              <div className="plan-price">
                US${plan.usd}
                <span className="plan-period">/plano</span>
              </div>
              {plan.discount && <span className="badge badge-forsale">{plan.discount}</span>}
              <ul className="plan-benefits">
                {(PLAN_BENEFITS[plan.id] || []).map((b) => (
                  <li key={b}>{b}</li>
                ))}
              </ul>
              <button
                className="btn btn-primary btn-block"
                data-testid={`buy-${plan.id}`}
                onClick={() => {
                  setSelectedPlan(plan);
                  setOrder(null);
                }}
              >
                Comprar
              </button>
            </div>
          ))}
        </div>
      )}

      {selectedPlan && !order && (
        <div className="card">
          <h3 className="section-title">Método de pagamento — {selectedPlan.label}</h3>
          <div className="method-options">
            {(['pix', 'usdt', 'auge'] as OrderMethod[]).map((m) => (
              <label className="method-option" key={m}>
                <input
                  type="radio"
                  name="method"
                  value={m}
                  checked={method === m}
                  onChange={() => setMethod(m)}
                />
                <span>{METHOD_LABELS[m]}</span>
              </label>
            ))}
          </div>
          <div className="actions">
            <button
              className="btn btn-primary"
              disabled={creating}
              onClick={() => void buy(selectedPlan)}
            >
              {creating ? 'Criando pedido…' : 'Continuar'}
            </button>
            <button className="btn btn-ghost" onClick={() => setSelectedPlan(null)}>
              Cancelar
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
