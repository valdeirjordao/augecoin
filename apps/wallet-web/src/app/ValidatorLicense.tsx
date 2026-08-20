import { useCallback, useEffect, useState } from 'react';
import { useAuth } from './AuthContext';
import {
  listOrders,
  issueLicense,
  getOverview,
  getDownloads,
  type Download,
} from '../services/validator';
import type { LicenseView, ValidatorOrder } from '../types';

const PLAN_LABELS: Record<string, string> = {
  monthly: 'Mensal',
  semiannual: 'Semestral',
  annual: 'Anual',
};

function statusBadge(order: ValidatorOrder) {
  switch (order.status) {
    case 'pending':
      return <span className="status-badge badge-forsale">Aguardando pagamento</span>;
    case 'paid':
      return <span className="status-badge badge-normal">Pago</span>;
    case 'issued':
      return <span className="status-badge badge-owned">Emitida</span>;
    default:
      return <span className="status-badge badge-unknown">{order.status}</span>;
  }
}

function licenseStatusBadge(s: string) {
  switch (s) {
    case 'active':
      return <span className="status-badge badge-normal">Ativa</span>;
    case 'expired':
      return <span className="status-badge badge-forsale">Expirada</span>;
    case 'suspended':
      return <span className="status-badge badge-giftpending">Suspensa</span>;
    default:
      return <span className="status-badge badge-unknown">{s}</span>;
  }
}

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export function ValidatorLicense() {
  const { user } = useAuth();
  const [orders, setOrders] = useState<ValidatorOrder[]>([]);
  const [licenses, setLicenses] = useState<LicenseView[]>([]);
  const [downloads, setDownloads] = useState<Record<string, Download>>({});
  const [newKey, setNewKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [o, ov, d] = await Promise.all([listOrders(), getOverview(), getDownloads()]);
      setOrders(o.orders);
      setLicenses(ov.licenses);
      setDownloads(d.downloads);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Erro ao carregar.');
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  if (!user) return null;

  async function emit(orderId: string) {
    setBusy(orderId);
    setError(null);
    try {
      const res = await issueLicense(orderId);
      setNewKey(res.license_key);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Erro ao emitir licença.');
    } finally {
      setBusy(null);
    }
  }

  function downloadButton(platform: string, label: string) {
    const d = downloads[platform];
    if (d && d.artifact_url) {
      return (
        <a className="btn" href={d.artifact_url} target="_blank" rel="noreferrer">
          {label} · v{d.version}
        </a>
      );
    }
    return (
      <button className="btn" disabled title="Release ainda não publicado">
        {label}
      </button>
    );
  }

  return (
    <div className="page">
      <div className="page-head">
        <h2>Minha Licença</h2>
        <p className="muted">Sua licença de validação e o download do software.</p>
      </div>

      {error && <p className="login-error">{error}</p>}

      {newKey && (
        <div className="card" style={{ borderColor: 'var(--color-success)' }}>
          <h3 className="section-title">Chave gerada — copie agora (exibida uma única vez)</h3>
          <div className="license-key mono" data-testid="license-key">
            {newKey}
          </div>
          <div className="actions" style={{ marginTop: 8 }}>
            <button
              className="btn"
              onClick={async () => {
                if (await copyText(newKey)) {
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                }
              }}
            >
              {copied ? 'Copiado ✓' : 'Copiar chave'}
            </button>
          </div>
        </div>
      )}

      <div className="section">
        <h3 className="section-title">Licenças</h3>
        {licenses.length === 0 ? (
          <div className="card">
            <p className="muted">
              Você ainda não possui uma licença.{' '}
              <a href="#/validator/plans">Comprar plano</a>
            </p>
          </div>
        ) : (
          <div className="account-list">
            {licenses.map((l) => (
              <div className="card" key={l.id}>
                <div className="kv">
                  <span className="kv-label">Licença</span>
                  <span className="kv-value mono">{l.license_key_prefix}…</span>
                </div>
                <div className="kv">
                  <span className="kv-label">Plano</span>
                  <span className="kv-value">{PLAN_LABELS[l.plan] || l.plan}</span>
                </div>
                <div className="kv">
                  <span className="kv-label">Status</span>
                  <span className="kv-value">{licenseStatusBadge(l.status)}</span>
                </div>
                <div className="kv">
                  <span className="kv-label">Expira em</span>
                  <span className="kv-value">{l.expires_at.slice(0, 10)}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="section">
        <h3 className="section-title">Download do software</h3>
        <div className="actions">
          {downloadButton('windows', 'Windows (MSI)')}
          {downloadButton('linux', 'Linux (DEB/AppImage)')}
        </div>
      </div>

      <div className="section">
        <h3 className="section-title">Pedidos</h3>
        {orders.length === 0 ? (
          <div className="card">
            <p className="muted">Nenhum pedido ainda.</p>
          </div>
        ) : (
          <div className="account-list">
            {orders.map((o) => (
              <div className="card" key={o.id}>
                <div className="kv">
                  <span className="kv-label">Plano</span>
                  <span className="kv-value">
                    {PLAN_LABELS[o.plan]} — US${o.amount_usd}
                  </span>
                </div>
                <div className="kv">
                  <span className="kv-label">Status</span>
                  <span className="kv-value">{statusBadge(o)}</span>
                </div>
                <div className="kv">
                  <span className="kv-label">Referência</span>
                  <span className="kv-value mono">{o.id.slice(0, 13)}…</span>
                </div>
                {o.status === 'paid' && (
                  <div className="actions" style={{ marginTop: 8 }}>
                    <button
                      className="btn btn-primary"
                      disabled={busy === o.id}
                      onClick={() => void emit(o.id)}
                    >
                      {busy === o.id ? 'Emitindo…' : 'Emitir licença'}
                    </button>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
