import { useCallback, useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useAuth } from './AuthContext';
import { getOverview } from '../services/validator';
import { formatAuge } from '../services/config';
import type { RewardBucket, ValidatorDetail } from '../types';

function fmtAuge(s: string | undefined): string {
  if (s === undefined) return '0 AUGE';
  try {
    return `${formatAuge(BigInt(s))} AUGE`;
  } catch {
    return '0 AUGE';
  }
}

function statusBadge(v: ValidatorDetail['validator']) {
  if (v.status === 'pending') return <span className="status-badge badge-unknown">Pendente</span>;
  if (v.status === 'suspended') return <span className="status-badge badge-giftpending">Suspenso</span>;
  if (v.status === 'revoked') return <span className="status-badge badge-unknown">Revogado</span>;
  return v.online ? (
    <span className="status-badge badge-normal">Online</span>
  ) : (
    <span className="status-badge badge-forsale">Offline</span>
  );
}

function Bars({ data, labels }: { data: number[]; labels: string[] }) {
  const max = Math.max(1, ...data);
  return (
    <div className="mini-bars">
      {data.map((v, i) => (
        <div className="mini-bar" key={labels[i]} title={`${labels[i]}: ${fmtAuge(String(v))}`}>
          <div className="mini-bar-fill" style={{ height: `${(v / max) * 100}%` }} />
          <span className="mini-bar-label">{labels[i]}</span>
        </div>
      ))}
    </div>
  );
}

function ValidatorCard({ detail }: { detail: ValidatorDetail }) {
  const { validator: v, rewards: r } = detail;
  const buckets: [string, RewardBucket][] = [
    ['Hora', r.hour],
    ['Dia', r.day],
    ['Semana', r.week],
    ['Mês', r.month],
    ['Ano', r.year],
    ['Total', r.total],
  ];

  return (
    <div className="card">
      <div className="kv">
        <span className="kv-label">Validador</span>
        <span className="kv-value">
          {v.augeid || '—'} <span className="mono">{v.public_key.slice(0, 10)}…</span>
        </span>
      </div>
      <div className="kv">
        <span className="kv-label">Status</span>
        <span className="kv-value">{statusBadge(v)}</span>
      </div>
      <div className="kv">
        <span className="kv-label">Uptime</span>
        <span className="kv-value">
          {Math.floor(v.uptime / 86400)}d {Math.floor((v.uptime % 86400) / 3600)}h
        </span>
      </div>
      <div className="kv">
        <span className="kv-label">Blocos produzidos</span>
        <span className="kv-value">{v.leadership}</span>
      </div>

      <div className="section-title" style={{ marginTop: 12 }}>
        AUGE por período
      </div>
      <Bars data={buckets.map(([, b]) => Number(b.auge))} labels={buckets.map(([l]) => l)} />
      <div className="kv">
        <span className="kv-label">AUGE total</span>
        <span className="kv-value">{fmtAuge(r.total.auge)}</span>
      </div>
      <div className="kv">
        <span className="kv-label">AUGEIDs total</span>
        <span className="kv-value">{r.total.augeids}</span>
      </div>
    </div>
  );
}

export function ValidatorDashboard() {
  const { user } = useAuth();
  const [validators, setValidators] = useState<ValidatorDetail[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const ov = await getOverview();
      setValidators(ov.validators);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Erro ao carregar.');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
    const t = setInterval(() => void reload(), 8000);
    return () => clearInterval(t);
  }, [reload]);

  if (!user) return null;

  return (
    <div className="page">
      <div className="page-head">
        <h2>Painel do Validador</h2>
        <p className="muted">Status e ganhos em tempo real.</p>
      </div>

      {error && <p className="login-error">{error}</p>}

      {loading ? (
        <p className="muted">Carregando…</p>
      ) : validators.length === 0 ? (
        <div className="card">
          <p className="muted">
            Nenhum validador ativado ainda. Baixe o software e ative sua licença.{' '}
            <Link to="/validator/license">Ver licença</Link>
          </p>
        </div>
      ) : (
        <div className="card-grid">
          {validators.map((d) => (
            <ValidatorCard key={d.validator.id} detail={d} />
          ))}
        </div>
      )}
    </div>
  );
}
