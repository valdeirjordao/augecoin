import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from './AuthContext';

export function Login() {
  const { login, register, loading, error, clearError } = useAuth();
  const navigate = useNavigate();
  const [mode, setMode] = useState<'login' | 'register'>('login');
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [localError, setLocalError] = useState('');
  const [justRegistered, setJustRegistered] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError('');
    clearError();
    setJustRegistered(false);
    if (!email.trim() || !password.trim()) {
      setLocalError('Informe e-mail e senha.');
      return;
    }
    if (mode === 'register' && !name.trim()) {
      setLocalError('Informe o nome.');
      return;
    }
    try {
      if (mode === 'register') {
        await register({ name, email, password });
        setJustRegistered(true);
      } else {
        await login(email, password);
      }
      navigate('/');
    } catch {
      /* error surfaced via context */
    }
  };

  return (
    <div className="auth-page">
      <div className="card auth-card">
        <h1 className="brand">
          <img src="/logo.svg" className="brand-logo" alt="" />
          <span className="brand-text">
            AUGECOIN<span className="brand-sub">Wallet</span>
          </span>
        </h1>
        <p className="muted center">
          Identidade da plataforma. Seu AUGEID é um ativo emitido pela blockchain — comprado ou recebido por doação.
        </p>

        <div className="tabs">
          <button className={mode === 'login' ? 'active' : ''} onClick={() => setMode('login')} data-testid="tab-login">
            Entrar
          </button>
          <button className={mode === 'register' ? 'active' : ''} onClick={() => setMode('register')} data-testid="tab-register">
            Cadastrar
          </button>
        </div>

        <form onSubmit={submit} className="form">
          {mode === 'register' && (
            <label className="field">
              <span className="field-label">Nome</span>
              <input value={name} onChange={(e) => setName(e.target.value)} data-testid="register-name" placeholder="Seu nome" />
            </label>
          )}
          <label className="field">
            <span className="field-label">E-mail</span>
            <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} data-testid="auth-email" placeholder="voce@exemplo.com" />
          </label>
          <label className="field">
            <span className="field-label">Senha</span>
            <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} data-testid="auth-password" placeholder="••••••••" />
          </label>

          {(localError || error) && (
            <p className="error" data-testid="auth-error">
              {localError || error}
            </p>
          )}

          <button className="btn btn-primary" type="submit" disabled={loading} data-testid="auth-submit">
            {loading ? 'Aguarde…' : mode === 'register' ? 'Criar conta' : 'Entrar'}
          </button>
        </form>

        {mode === 'register' && (
          <p className="muted small center" data-testid="register-note">
            O cadastro cria apenas sua Conta da Plataforma — mas <strong>não</strong> cria AUGEID.
          </p>
        )}

        {justRegistered && (
          <div className="info-box" data-testid="registered-note">
            Sua conta foi criada. Você ainda não possui um AUGEID — receba uma doação ou compre um no marketplace para ativar sua Carteira Blockchain.
          </div>
        )}
      </div>
    </div>
  );
}
