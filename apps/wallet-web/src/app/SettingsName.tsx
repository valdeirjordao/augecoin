import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useAuth } from './AuthContext';
import { useAccount } from '../hooks/useAccount';
import { useToast } from './ToastContext';
import { getAccount, isNameAvailable } from '../services/rpc';
import { submitChangeAccountInfo } from '../services/operations';
import { CONFIG } from '../services/config';

export function SettingsName() {
  const { session } = useAuth();
  const { owned } = useAccount();
  const { toast } = useToast();
  const [params] = useSearchParams();

  const [account, setAccount] = useState<number | ''>(params.get('account') ? Number(params.get('account')) : '');
  const [name, setName] = useState('');
  const [status, setStatus] = useState<'idle' | 'checking' | 'available' | 'taken'>('idle');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!name.trim()) {
      setStatus('idle');
      return;
    }
    let active = true;
    setStatus('checking');
    const t = setTimeout(async () => {
      const available = await isNameAvailable(name);
      if (active) setStatus(available ? 'available' : 'taken');
    }, 300);
    return () => {
      active = false;
      clearTimeout(t);
    };
  }, [name]);

  const submit = async () => {
    if (!session || account === '') return;
    const acc = owned.find((a) => a.account_number === account);
    if (!acc) return;
    const current = await getAccount(acc.account_number);
    setBusy(true);
    try {
      const res = await submitChangeAccountInfo(session, {
        account: acc.account_number,
        nOperation: current.n_operation,
        fee: CONFIG.MIN_FEE_AUGESAT,
        newPublicKeyHex: current.account_key_ed_hex,
        newName: name.trim() || null,
        newType: current.account_type,
        newAccountDataHex: current.account_data_hex,
        newAccountSealHex: current.account_seal_hex,
      });
      if (res.accepted) {
        toast('Nome atualizado.', 'success');
        setName('');
      } else {
        toast(res.error ?? 'Falha ao alterar nome.', 'error');
      }
    } catch (e) {
      toast(e instanceof Error ? e.message : 'Falha ao alterar nome.', 'error');
    } finally {
      setBusy(false);
    }
  };

  const canSubmit = status === 'available' && name.trim() && account !== '';

  return (
    <div className="page">
      <div className="page-head">
        <h2>Alterar nome</h2>
        <p className="muted">Registre ou altere o nome de um AUGEID (verificado no name_index).</p>
      </div>

      <div className="card form">
        <label className="field">
          <span className="field-label">AUGEID</span>
          <select value={account} onChange={(e) => setAccount(Number(e.target.value))} className="text-input" data-testid="name-account-select">
            <option value="">Selecione…</option>
            {owned.map((a) => (
              <option key={a.account_number} value={a.account_number}>
                AUGEID #{a.account_number} {a.name ? `(${a.name})` : ''}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span className="field-label">Novo nome</span>
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder="ex.: CarlosPay" className="text-input" data-testid="name-input" />
        </label>

        {status === 'checking' && <p className="muted">Verificando disponibilidade…</p>}
        {status === 'available' && (
          <p className="success" data-testid="name-available">
            Nome disponível.
          </p>
        )}
        {status === 'taken' && (
          <p className="error" data-testid="name-taken">
            Nome já em uso.
          </p>
        )}

        <button className="btn btn-primary" onClick={submit} disabled={busy || !canSubmit} data-testid="name-submit">
          {busy ? 'Salvando…' : 'Salvar nome'}
        </button>
      </div>
    </div>
  );
}
