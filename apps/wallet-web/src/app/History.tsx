import { useEffect, useState } from 'react';
import { useLinkedWallets } from '../hooks/useLinkedWallets';
import { getNodeStatus } from '../services/rpc';
import type { NodeStatus } from '../types';

/**
 * Histórico — on-chain activity summary for the user's linked wallets.
 * Reads only official RPCs (`getaccount`, `nodestatus`); never duplicates
 * consensus rules and never fabricates a per-account transaction index.
 */
export function History() {
  const { wallets } = useLinkedWallets();
  const [node, setNode] = useState<NodeStatus | null>(null);

  useEffect(() => {
    let active = true;
    getNodeStatus()
      .then((s) => {
        if (active) setNode(s);
      })
      .catch(() => {
        /* ignore */
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <div className="page">
      <div className="page-head">
        <h2>Histórico</h2>
        <p className="muted">
          Atividade on-chain das suas carteiras. Altura atual: {node ? `#${node.current_height}` : '—'}
        </p>
      </div>

      {wallets.length === 0 ? (
        <div className="empty-state">
          <p className="muted">Nenhuma carteira vinculada.</p>
        </div>
      ) : (
        <div className="table-wrap" data-testid="history-list">
          <table className="data-table">
            <thead>
              <tr>
                <th>AUGEID</th>
                <th>Nome</th>
                <th>Operações (n_operation)</th>
                <th>Última atividade (bloco)</th>
              </tr>
            </thead>
            <tbody>
              {wallets.map((w) => (
                <tr key={w.account_number} data-testid={`history-${w.account_number}`}>
                  <td className="mono">#{w.account_number}</td>
                  <td>{w.account?.name ?? 'Sem nome'}</td>
                  <td>{w.account?.n_operation ?? '—'}</td>
                  <td>{w.account ? `#${w.account.updated_on_block_active_mode}` : '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
