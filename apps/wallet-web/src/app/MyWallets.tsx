import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { useInventory } from '../hooks/useInventory';
import { useLinkedWallets } from '../hooks/useLinkedWallets';
import { StatusBadge } from '../components/StatusBadge';
import { Modal, Skeleton } from '../components/Modal';
import { useToast } from './ToastContext';
import { directory } from '../services/platform';
import { formatAuge } from '../services/config';
import type { DirectoryMember } from '../types';

type Action =
  | { kind: 'sell'; accountNumber: number }
  | { kind: 'gift'; accountNumber: number }
  | { kind: 'transfer'; accountNumber: number }
  | null;

/**
 * Minhas Carteiras — every AUGEID linked to the user (wallet registry), with
 * its on-chain name, balance and state. Built for multiple AUGEIDs.
 */
export function MyWallets() {
  const { wallets, loading, error } = useLinkedWallets();
  const { sell, cancel, gift, transfer, busy } = useInventory();
  const { toast } = useToast();

  const [action, setAction] = useState<Action>(null);
  const [price, setPrice] = useState('');
  const [memberId, setMemberId] = useState('');
  const [members, setMembers] = useState<DirectoryMember[]>([]);

  useEffect(() => {
    let active = true;
    directory()
      .then((m) => {
        if (active) setMembers(m);
      })
      .catch(() => {
        /* ignore directory errors */
      });
    return () => {
      active = false;
    };
  }, []);

  const run = async (fn: () => Promise<boolean>, successMsg: string) => {
    const ok = await fn();
    if (ok) {
      toast(successMsg, 'success');
      setAction(null);
    } else {
      toast('Operação rejeitada.', 'error');
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Minhas Carteiras</h2>
        <p className="muted">Todas as carteiras blockchain vinculadas à sua conta.</p>
      </div>

      {error && <p className="error">{error}</p>}

      {loading && wallets.length === 0 ? (
        <Skeleton rows={3} />
      ) : wallets.length === 0 ? (
        <div className="empty-state">
          <p className="muted">Nenhuma carteira vinculada.</p>
        </div>
      ) : (
        <div className="table-wrap" data-testid="my-wallets-list">
          <table className="data-table">
            <thead>
              <tr>
                <th>AUGEID</th>
                <th>Nome</th>
                <th>Saldo</th>
                <th>Status</th>
                <th>Ações</th>
              </tr>
            </thead>
            <tbody>
              {wallets.map((w) => {
                const a = w.account;
                return (
                  <tr key={w.account_number} data-testid={`wallet-row-${w.account_number}`}>
                    <td className="mono">#{w.account_number}</td>
                    <td>{a?.name ?? 'Sem nome'}</td>
                    <td className="price">{a ? formatAuge(a.balance) : '—'}</td>
                    <td>{a ? <StatusBadge state={a.state} /> : '—'}</td>
                    <td>
                      <div className="actions">
                        {a?.state === 'ForSale' && (
                          <button
                            className="btn btn-danger"
                            onClick={() => run(() => cancel(w.account_number), 'Venda cancelada.')}
                            disabled={busy}
                            data-testid={`cancel-${w.account_number}`}
                          >
                            Cancelar venda
                          </button>
                        )}
                        {(a?.state === 'Reserved' || a?.state === 'Owned' || a?.state === 'Normal') && (
                          <>
                            <button className="btn" onClick={() => setAction({ kind: 'sell', accountNumber: w.account_number })} data-testid={`sell-${w.account_number}`}>
                              Vender
                            </button>
                            <button className="btn" onClick={() => setAction({ kind: 'gift', accountNumber: w.account_number })} data-testid={`gift-${w.account_number}`}>
                              Doar
                            </button>
                            <button className="btn" onClick={() => setAction({ kind: 'transfer', accountNumber: w.account_number })} data-testid={`transfer-${w.account_number}`}>
                              Transferir
                            </button>
                            <Link className="btn" to={`/settings/name?account=${w.account_number}`} data-testid={`rename-${w.account_number}`}>
                              Alterar Nome
                            </Link>
                          </>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      <Modal open={action?.kind === 'sell'} title="Vender AUGEID" onClose={() => setAction(null)}>
        {action?.kind === 'sell' && (
          <div className="form" data-testid="sell-modal">
            <p>
              Vender <strong>AUGEID #{action.accountNumber}</strong>
            </p>
            <label className="field">
              <span className="field-label">Preço (AUGE)</span>
              <input value={price} onChange={(e) => setPrice(e.target.value)} data-testid="sell-price" placeholder="0.00" />
            </label>
            <button
              className="btn btn-primary"
              onClick={() => run(() => sell(action.accountNumber, Math.round(Number(price) * 1e8)), 'AUGEID colocado à venda.')}
              disabled={busy || !price}
              data-testid="confirm-sell"
            >
              Colocar à venda
            </button>
          </div>
        )}
      </Modal>

      <Modal
        open={action?.kind === 'gift' || action?.kind === 'transfer'}
        title={action?.kind === 'gift' ? 'Doar AUGEID' : 'Transferir AUGEID'}
        onClose={() => setAction(null)}
      >
        {action && (action.kind === 'gift' || action.kind === 'transfer') && (
          <div className="form" data-testid="member-modal">
            <p>
              <strong>AUGEID #{action.accountNumber}</strong>
            </p>
            <label className="field">
              <span className="field-label">Destinatário</span>
              <select value={memberId} onChange={(e) => setMemberId(e.target.value)} data-testid="member-select">
                <option value="">Selecione um membro…</option>
                {members.map((m) => (
                  <option key={m.id} value={m.public_key_hex}>
                    {m.display_name} ({m.email})
                  </option>
                ))}
              </select>
            </label>
            <button
              className="btn btn-primary"
              onClick={() =>
                run(
                  () =>
                    action.kind === 'gift'
                      ? gift(action.accountNumber, memberId)
                      : transfer(action.accountNumber, memberId),
                  action.kind === 'gift' ? 'Doação enviada.' : 'Transferência enviada.',
                )
              }
              disabled={busy || !memberId}
              data-testid="confirm-member-op"
            >
              Confirmar
            </button>
          </div>
        )}
      </Modal>
    </div>
  );
}
