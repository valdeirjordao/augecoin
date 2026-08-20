import { useGifts } from '../hooks/useGifts';
import { GiftCard } from '../components/GiftCard';
import { Skeleton } from '../components/Modal';
import { useToast } from './ToastContext';

export function Gifts() {
  const { gifts, loading, busy, error, accept } = useGifts();
  const { toast } = useToast();

  const acceptGift = async (accountNumber: number) => {
    const ok = await accept(accountNumber);
    if (ok) {
      toast(`AUGEID #${accountNumber} aceito.`, 'success');
    } else {
      toast('Falha ao aceitar presente.', 'error');
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Receber Doação</h2>
        <p className="muted">AUGEIDs doados para você, aguardando aceite. Aceitar ativa sua Carteira Blockchain automaticamente.</p>
      </div>

      {error && <p className="error">{error}</p>}

      {loading && gifts.length === 0 ? (
        <Skeleton rows={3} />
      ) : gifts.length === 0 ? (
        <div className="empty-state">
          <p className="muted">Nenhum presente pendente.</p>
        </div>
      ) : (
        <div className="card-grid" data-testid="gifts-list">
          {gifts.map((g) => (
            <GiftCard key={g.account_number} gift={g} onAccept={() => acceptGift(g.account_number)} busy={busy} />
          ))}
        </div>
      )}
    </div>
  );
}
