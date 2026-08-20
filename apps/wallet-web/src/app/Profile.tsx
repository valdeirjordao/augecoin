import { useState } from 'react';
import { useAuth } from './AuthContext';
import { useLinkedWallets } from '../hooks/useLinkedWallets';
import { PlatformAccountCard } from '../components/PlatformAccountCard';
import { BlockchainWalletCard } from '../components/BlockchainWalletCard';
import { EmptyWalletCard } from '../components/EmptyWalletCard';
import { useToast } from './ToastContext';

/**
 * Perfil — shows ONLY platform information, visually separated from the
 * blockchain wallets section. The two layers are never mixed.
 */
export function Profile() {
  const { user, preferences, updateProfile, updatePreferences } = useAuth();
  const { wallets, hasWallet } = useLinkedWallets();
  const { toast } = useToast();

  const [displayName, setDisplayName] = useState(user?.display_name ?? '');
  const [notifications, setNotifications] = useState(preferences?.notifications ?? true);
  const [saving, setSaving] = useState(false);

  if (!user) return null;

  const saveProfile = async () => {
    setSaving(true);
    try {
      await updateProfile(displayName.trim());
      await updatePreferences({ notifications });
      toast('Perfil atualizado.', 'success');
    } catch (e) {
      toast(e instanceof Error ? e.message : 'Falha ao salvar perfil.', 'error');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Perfil</h2>
        <p className="muted">Suas informações da plataforma — separadas das carteiras blockchain.</p>
      </div>

      <section className="profile-section" data-testid="profile-platform">
        <div className="section-title layer-chip layer-platform">Conta da Plataforma</div>
        <PlatformAccountCard user={user} />

        <div className="card form">
          <label className="field">
            <span className="field-label">Nome de exibição</span>
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              className="text-input"
              data-testid="profile-display-name"
            />
          </label>
          <label className="field">
            <span className="field-label">E-mail</span>
            <input value={user.email} disabled className="text-input" />
          </label>
          <label className="field checkbox-field">
            <input
              type="checkbox"
              checked={notifications}
              onChange={(e) => setNotifications(e.target.checked)}
              data-testid="profile-notifications"
            />
            <span>Receber notificações</span>
          </label>
          <button className="btn btn-primary" onClick={saveProfile} disabled={saving} data-testid="profile-save">
            {saving ? 'Salvando…' : 'Salvar'}
          </button>
        </div>
      </section>

      <section className="profile-section" data-testid="profile-wallets">
        <div className="section-title layer-chip layer-wallet">Carteiras Blockchain</div>
        {hasWallet ? (
          <div className="account-list">
            {wallets.map((w) =>
              w.account ? <BlockchainWalletCard key={w.account_number} account={w.account} /> : null,
            )}
          </div>
        ) : (
          <EmptyWalletCard />
        )}
      </section>
    </div>
  );
}
