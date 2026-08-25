import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

type Screen = 'welcome' | 'create' | 'confirm' | 'import' | 'password' | 'home';

export default function App() {
  const [screen, setScreen] = useState<Screen>('welcome');
  const [mnemonic, setMnemonic] = useState('');
  const [accounts, setAccounts] = useState([{ num: 0, balance: '0.0' }]);

  const handleCreate = useCallback(async () => {
    try {
      const info: any = await invoke('create_wallet');
      setMnemonic(info.mnemonic);
      setScreen('create');
    } catch (e) {
      console.error(e);
    }
  }, []);

  const handleImport = useCallback(() => setScreen('import'), []);

  const handleMnemonicReady = useCallback((m: string) => {
    setMnemonic(m);
    setScreen('password');
  }, []);

  const handlePasswordSet = useCallback(async () => {
    setScreen('home');
  }, []);

  if (screen === 'welcome') {
    return (
      <div className="screen">
        <h1>AUGECOIN Desktop</h1>
        <p className="subtitle">Carteira Segura — Rust nativo</p>
        <div className="actions">
          <button onClick={handleCreate} className="primary">
            Criar Nova Carteira
          </button>
          <button onClick={handleImport}>Importar Carteira Existente</button>
        </div>
      </div>
    );
  }

  if (screen === 'create') {
    return <CreateScreen mnemonic={mnemonic} onNext={() => setScreen('confirm')} />;
  }

  if (screen === 'confirm') {
    return <ConfirmScreen mnemonic={mnemonic} onConfirmed={() => handleMnemonicReady(mnemonic)} />;
  }

  if (screen === 'import') {
    return <ImportScreen onImported={(m) => handleMnemonicReady(m)} />;
  }

  if (screen === 'password') {
    return <PasswordScreen mnemonic={mnemonic} onPasswordSet={handlePasswordSet} />;
  }

  return <HomeScreen accounts={accounts} />;
}

function CreateScreen({ mnemonic, onNext }: { mnemonic: string; onNext: () => void }) {
  const [ack, setAck] = useState(false);
  return (
    <div className="screen">
      <h2>Palavras de Recuperação</h2>
      <div className="warning">
        Guarde estas palavras. Exibidas APENAS UMA VEZ.
      </div>
      <div className="mnemonic-box">
        {mnemonic.split(' ').map((w, i) => (
          <span key={i} className="mnemonic-word">{i + 1}. {w}</span>
        ))}
      </div>
      <label className="checkbox-label">
        <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
        Eu anotei minhas palavras
      </label>
      <button onClick={onNext} disabled={!ack} className="primary">Continuar</button>
    </div>
  );
}

function ConfirmScreen({ mnemonic, onConfirmed }: { mnemonic: string; onConfirmed: () => void }) {
  const words = mnemonic.split(' ');
  const ri = Math.floor(Math.random() * words.length);
  const [input, setInput] = useState('');
  const [err, setErr] = useState('');
  const check = () => {
    if (input.trim().toLowerCase() === words[ri].toLowerCase()) onConfirmed();
    else setErr(`Palavra #${ri + 1} incorreta.`);
  };
  return (
    <div className="screen">
      <h2>Confirme sua Seed</h2>
      <p>Digite a palavra <strong>#{ri + 1}</strong>:</p>
      <input className="text-input" value={input} onChange={(e) => { setInput(e.target.value); setErr(''); }} />
      {err && <p className="error">{err}</p>}
      <button onClick={check} className="primary">Verificar</button>
    </div>
  );
}

function ImportScreen({ onImported }: { onImported: (m: string) => void }) {
  const [input, setInput] = useState('');
  const [err, setErr] = useState('');
  const handle = async () => {
    try {
      await invoke('import_wallet', { mnemonic: input.trim() });
      onImported(input.trim());
    } catch { setErr('Seed inválida.'); }
  };
  return (
    <div className="screen">
      <h2>Importar Carteira</h2>
      <textarea className="text-input" rows={3} value={input} onChange={(e) => { setInput(e.target.value); setErr(''); }} />
      {err && <p className="error">{err}</p>}
      <button onClick={handle} className="primary">Importar</button>
    </div>
  );
}

function PasswordScreen({ mnemonic, onPasswordSet }: { mnemonic: string; onPasswordSet: () => void }) {
  const [pw, setPw] = useState('');
  const [cf, setCf] = useState('');
  const [err, setErr] = useState('');
  const save = async () => {
    if (pw.length < 8) { setErr('Mínimo 8 caracteres.'); return; }
    if (pw !== cf) { setErr('Senhas não conferem.'); return; }
    try {
      await invoke('keystore_save', { mnemonic, password: pw });
      onPasswordSet();
    } catch (e: any) { setErr(e.toString()); }
  };
  return (
    <div className="screen">
      <h2>Senha do Keystore</h2>
      <input className="text-input" type="password" placeholder="Senha" value={pw} onChange={(e) => setPw(e.target.value)} />
      <input className="text-input" type="password" placeholder="Confirmar" value={cf} onChange={(e) => setCf(e.target.value)} />
      {err && <p className="error">{err}</p>}
      <button onClick={save} className="primary">Proteger com Senha</button>
    </div>
  );
}

function HomeScreen({ accounts }: { accounts: Array<{ num: number; balance: string }> }) {
  const [tab, setTab] = useState<'balance' | 'send' | 'receive'>('balance');
  return (
    <div className="screen">
      <h2>Carteira AUGECOIN</h2>
      <div className="balance-card">
        <span className="balance-label">Saldo Total</span>
        <span className="balance-value">{accounts[0]?.balance ?? '0.0'} AUGE</span>
      </div>
      <div className="tabs">
        <button onClick={() => setTab('balance')} className={tab === 'balance' ? 'active' : ''}>Saldo</button>
        <button onClick={() => setTab('send')} className={tab === 'send' ? 'active' : ''}>Enviar</button>
        <button onClick={() => setTab('receive')} className={tab === 'receive' ? 'active' : ''}>Receber</button>
      </div>
      {tab === 'balance' && (
        <div className="tab-content">
          {accounts.map(a => (
            <div key={a.num} className="account-row">
              <span>Conta #{a.num}</span><span>{a.balance} AUGE</span>
            </div>
          ))}
        </div>
      )}
      {tab === 'send' && (
        <div className="tab-content">
          <input className="text-input" placeholder="Conta destino" />
          <input className="text-input" placeholder="Quantidade AUGE" />
          <button className="primary">Enviar</button>
        </div>
      )}
      {tab === 'receive' && (
        <div className="tab-content">
          <p>Endereço: <code>3Fb2QT3ngwCVZPSK9KEKVo241d7oJMxw91HzpMo</code></p>
        </div>
      )}
    </div>
  );
}
