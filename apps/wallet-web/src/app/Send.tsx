import { useState } from 'react';
import { useAuth } from './AuthContext';
import { useAccount } from '../hooks/useAccount';
import { useToast } from './ToastContext';
import { resolveName } from '../services/rpc';
import { submitTransfer } from '../services/operations';
import { CONFIG, formatAuge } from '../services/config';
import type { AccountInfo } from '../types';

export function Send() {
  const { session } = useAuth();
  const { owned } = useAccount();
  const { toast } = useToast();

  const [from, setFrom] = useState<number | ''>('');
  const [to, setTo] = useState('');
  const [amount, setAmount] = useState('');
  const [resolved, setResolved] = useState<AccountInfo | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);

  const bech32 = /^auge1/i.test(to.trim());

  const onToChange = (v: string) => {
    setTo(v);
    setResolved(null);
    if (/^auge1/i.test(v.trim())) {
      setError('Endereços Bech32 não são mais aceitos.');
    } else {
      setError('');
    }
  };

  const resolve = async () => {
    setError('');
    if (!to.trim()) {
      setError('Informe um AUGEID (número) ou nome.');
      return;
    }
    if (bech32) {
      setError('Endereços Bech32 não são mais aceitos.');
      return;
    }
    try {
      const acc = await resolveName(to);
      if (!acc) {
        setResolved(null);
        setError('AUGEID ou nome não encontrado.');
        return;
      }
      setResolved(acc);
    } catch {
      setError('Falha ao resolver destino.');
    }
  };

  const submit = async () => {
    if (!session || from === '') return;
    const sender = owned.find((a) => a.account_number === from);
    if (!sender || !resolved) return;
    const augesat = Math.round(Number(amount) * 1e8);
    if (!amount || Number.isNaN(augesat) || augesat <= 0) {
      setError('Quantidade inválida.');
      return;
    }
    setBusy(true);
    setError('');
    try {
      const res = await submitTransfer(session, {
        sender: sender.account_number,
        nOperation: sender.n_operation,
        to: resolved.account_number,
        amount: augesat,
        fee: CONFIG.MIN_FEE_AUGESAT,
      });
      if (res.accepted) {
        toast(`Enviado ${formatAuge(augesat)} AUGE para AUGEID #${resolved.account_number}.`, 'success');
        setAmount('');
        setResolved(null);
        setTo('');
      } else {
        setError(res.error ?? 'Transação rejeitada.');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Falha ao enviar.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Enviar AUGE</h2>
        <p className="muted">Destinatário por número de AUGEID ou nome.</p>
      </div>

      <div className="card form">
        <label className="field">
          <span className="field-label">AUGEID de origem</span>
          <select value={from} onChange={(e) => setFrom(Number(e.target.value))} className="text-input" data-testid="send-from">
            <option value="">Selecione…</option>
            {owned.map((a) => (
              <option key={a.account_number} value={a.account_number}>
                AUGEID #{a.account_number} — {formatAuge(a.balance)} AUGE
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span className="field-label">Destinatário (AUGEID ou nome)</span>
          <input value={to} onChange={(e) => onToChange(e.target.value)} placeholder="154650 ou CarlosPay" className="text-input" data-testid="send-to" />
        </label>

        {resolved && (
          <div className="info-box" data-testid="send-resolved">
            Destino: AUGEID #{resolved.account_number}
            {resolved.name ? ` (${resolved.name})` : ''}
          </div>
        )}

        {!resolved && to.trim() && !bech32 && (
          <button className="btn" onClick={resolve} data-testid="resolve-btn">
            Resolver destino
          </button>
        )}

        <label className="field">
          <span className="field-label">Quantidade (AUGE)</span>
          <input value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="0.00" className="text-input" data-testid="send-amount" />
        </label>

        {error && (
          <p className="error" data-testid="send-error">
            {error}
          </p>
        )}

        <button className="btn btn-primary" onClick={submit} disabled={busy || !resolved || from === ''} data-testid="send-submit">
          {busy ? 'Enviando…' : 'Enviar'}
        </button>
      </div>
    </div>
  );
}
