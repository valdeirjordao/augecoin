import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface SavedState {
  license_key: string;
  augeid: string | null;
  public_key: string;
  machine_id: string;
  activated_at: string | null;
  app_version: string | null;
}

interface ValidatorView {
  id: string;
  public_key: string;
  augeid: string | null;
  ip: string | null;
  os: string | null;
  online: boolean;
  status: 'pending' | 'active' | 'suspended' | 'revoked';
  uptime: number;
  blocks: number;
  leadership: number;
  total_rewards: string;
  license_expires_at: string | null;
}

interface RewardBucket {
  auge: string;
  augeids: number;
  blocks: number;
}

interface RewardSummary {
  hour: RewardBucket;
  day: RewardBucket;
  week: RewardBucket;
  month: RewardBucket;
  year: RewardBucket;
  total: RewardBucket;
}

interface NodeStatus {
  running: boolean;
  syncing: boolean;
  current_height: number;
  peers_connected: number;
  validator_id: number;
  chain_id: number;
  uptime_seconds: number;
  error: string | null;
}

interface DashboardData {
  state: SavedState;
  validator: ValidatorView;
  rewards: RewardSummary;
  node: NodeStatus;
}

interface ActivationSummary {
  state: SavedState;
  validator: ValidatorView;
  heartbeat_interval_seconds: number;
}

interface UpdateInfo {
  current_version: string;
  latest_version: string | null;
  up_to_date: boolean;
  artifact_url: string | null;
  signature_valid: boolean;
}

function fmtAuge(s: string | undefined): string {
  if (!s) return '0 AUGE';
  try {
    const n = BigInt(s);
    const per = 100000000n;
    const int = n / per;
    const frac = (n % per).toString().padStart(8, '0').replace(/0+$/, '');
    return `${int}${frac ? '.' + frac : ''} AUGE`;
  } catch {
    return '0 AUGE';
  }
}

function fmtUptime(sec: number): string {
  const d = Math.floor(sec / 86400);
  const h = Math.floor((sec % 86400) / 3600);
  return d > 0 ? `${d}d ${h}h` : `${h}h ${Math.floor((sec % 3600) / 60)}m`;
}

function statusBadge(v: ValidatorView) {
  if (v.status === 'pending') return <span className="badge badge-gray">Pendente</span>;
  if (v.status === 'suspended') return <span className="badge badge-amber">Suspenso</span>;
  if (v.status === 'revoked') return <span className="badge badge-red">Revogado</span>;
  return v.online ? <span className="badge badge-green">Online</span> : <span className="badge badge-amber">Offline</span>;
}

export default function App() {
  const [state, setState] = useState<SavedState | null>(null);
  const [loading, setLoading] = useState(true);
  const [activating, setActivating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<SavedState>('get_state')
      .then((s) => setState(s.license_key ? s : null))
      .catch(() => setState(null))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div className="screen"><p className="muted">Carregando…</p></div>;

  return state ? (
    <Dashboard state={state} onReset={() => setState(null)} />
  ) : (
    <Activation
      onActivated={(summary) => setState(summary.state)}
      activating={activating}
      setActivating={setActivating}
      error={error}
      setError={setError}
    />
  );
}

function Activation({
  onActivated,
  activating,
  setActivating,
  error,
  setError,
}: {
  onActivated: (summary: ActivationSummary) => void;
  activating: boolean;
  setActivating: (v: boolean) => void;
  error: string | null;
  setError: (v: string | null) => void;
}) {
  const [license, setLicense] = useState('');
  const [augeid, setAugeid] = useState('');

  const submit = useCallback(async () => {
    setError(null);
    setActivating(true);
    try {
      const summary = await invoke<ActivationSummary>('activate', {
        licenseKey: license.trim(),
        augeid: augeid.trim() || null,
      });
      onActivated(summary);
    } catch (e) {
      setError(String(e));
    } finally {
      setActivating(false);
    }
  }, [license, augeid, onActivated, setActivating, setError]);

  return (
    <div className="screen">
      <h1>AUGECOIN Validador</h1>
      <p className="muted">Ative seu validador em segundos. Sua chave privada fica apenas nesta máquina.</p>

      <div className="card">
        <h2>Ativação</h2>
        {error && <div className="error" style={{ marginTop: 12 }}>{error}</div>}
        <div style={{ marginTop: 16, display: 'flex', flexDirection: 'column', gap: 14 }}>
          <div>
            <label className="label">Licença</label>
            <input className="input" value={license} onChange={(e) => setLicense(e.target.value)} placeholder="8F2K-X91M-A7QP-5NLD-3R8C-HJ4T-Z6WV-PQ2X" />
          </div>
          <div>
            <label className="label">AUGEID (opcional)</label>
            <input className="input" value={augeid} onChange={(e) => setAugeid(e.target.value)} placeholder="destino das recompensas" />
          </div>
          <button className="primary" disabled={activating || !license.trim()} onClick={submit}>
            {activating ? 'Ativando…' : 'Finalizar Ativação'}
          </button>
          {activating && (
            <div className="progress">
              <span className="muted">Validando licença, registrando máquina e iniciando sincronização…</span>
              <div className="bar"><div style={{ width: '70%' }} /></div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function Dashboard({ state, onReset }: { state: SavedState; onReset: () => void }) {
  const [data, setData] = useState<DashboardData | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const lastHeight = useRef<number | null>(null);
  const [leading, setLeading] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const d = await invoke<DashboardData>('get_dashboard');
      setData(d);
      if (lastHeight.current !== null && d.node.current_height > lastHeight.current) {
        setLeading(true);
        setTimeout(() => setLeading(false), 4000);
      }
      lastHeight.current = d.node.current_height;
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 8000);
    return () => clearInterval(t);
  }, [refresh]);

  const checkUpdate = useCallback(async () => {
    const u = await invoke<UpdateInfo>('check_update', { currentVersion: state.app_version || '0.1.0' });
    setUpdate(u);
  }, [state.app_version]);

  const reset = useCallback(async () => {
    await invoke('clear_state');
    onReset();
  }, [onReset]);

  const v = data?.validator;
  const r = data?.rewards;
  const node = data?.node;

  return (
    <div className="screen">
      <div className="row" style={{ borderBottom: 'none' }}>
        <h1>AUGECOIN Validador</h1>
        <button onClick={reset}>Desativar</button>
      </div>

      {error && <div className="error">{error}</div>}

      {leading && (
        <div className="leader">
          <h2 style={{ color: '#f97316' }}>Você é o Validador Chefe</h2>
          <p className="muted">Produzindo blocos e contabilizando ganhos…</p>
        </div>
      )}

      <div className="stat-grid">
        <div className="stat">
          <span className="label">Status</span>
          <div className="v">{v ? statusBadge(v) : '—'}</div>
        </div>
        <div className="stat">
          <span className="label">Sincronizado</span>
          <div className="v">{node?.syncing ? <span className="badge badge-amber">sincronizando</span> : <span className="badge badge-green">em dia</span>}</div>
        </div>
        <div className="stat">
          <span className="label">Último bloco</span>
          <div className="v">{node?.current_height ?? '—'}</div>
        </div>
        <div className="stat">
          <span className="label">Peers</span>
          <div className="v">{node?.peers_connected ?? '—'}</div>
        </div>
        <div className="stat">
          <span className="label">Ganhos hoje</span>
          <div className="v">{fmtAuge(r?.day.auge)}</div>
        </div>
        <div className="stat">
          <span className="label">Ganhos totais</span>
          <div className="v">{fmtAuge(r?.total.auge)}</div>
        </div>
      </div>

      <div className="card">
        <h2>Licença</h2>
        <div className="row"><span className="kv-k">Chave</span><span className="kv-v mono">{state.license_key}</span></div>
        <div className="row"><span className="kv-k">AUGEID</span><span className="kv-v mono">{state.augeid || '—'}</span></div>
        <div className="row"><span className="kv-k">Chave pública</span><span className="kv-v mono">{state.public_key.slice(0, 24)}…</span></div>
        <div className="row"><span className="kv-k">Expiração</span><span className="kv-v">{v?.license_expires_at?.slice(0, 10) || '—'}</span></div>
        <div className="row"><span className="kv-k">Uptime</span><span className="kv-v">{node ? fmtUptime(node.uptime_seconds) : '—'}</span></div>
      </div>

      <div className="card">
        <h2>Ganhos</h2>
        <div className="row"><span className="kv-k">AUGE hoje</span><span className="kv-v">{fmtAuge(r?.day.auge)}</span></div>
        <div className="row"><span className="kv-k">AUGE na semana</span><span className="kv-v">{fmtAuge(r?.week.auge)}</span></div>
        <div className="row"><span className="kv-k">AUGE no mês</span><span className="kv-v">{fmtAuge(r?.month.auge)}</span></div>
        <div className="row"><span className="kv-k">AUGEIDs acumulados</span><span className="kv-v">{r?.total.augeids ?? 0}</span></div>
        <div className="row"><span className="kv-k">Blocos liderados</span><span className="kv-v">{v?.leadership ?? 0}</span></div>
      </div>

      <div className="card">
        <h2>Atualizações (OTA)</h2>
        <button onClick={checkUpdate}>Verificar atualização</button>
        {update && (
          <div style={{ marginTop: 12 }}>
            {update.up_to_date || !update.latest_version ? (
              <p className="muted">Você está na versão mais recente ({update.current_version}).</p>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                <p>
                  Nova versão <strong>{update.latest_version}</strong> disponível.
                </p>
                <p className={update.signature_valid ? 'success' : 'error'}>
                  {update.signature_valid
                    ? 'Assinatura verificada ✓ (instalação segura).'
                    : 'Assinatura INVÁLIDA — não instalar.'}
                </p>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
