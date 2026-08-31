import { useState } from 'react';
import { WalletConnection, WalletContext } from './lib/augecoin';
import { SmartContracts } from './sections/SmartContracts';

const LS_ENDPOINT = 'auge_wallet_endpoint';
const LS_APIKEY = 'auge_wallet_apikey';

function Dashboard({ connection }: { connection: WalletConnection }) {
  const [account, setAccount] = useState('');
  const [info, setInfo] = useState<{ status: unknown; balance: unknown } | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    setError(null);
    try {
      const status = await connection.rpc.getNodeStatus();
      const balance =
        account !== '' ? await connection.rpc.getAccount(Number(account)) : null;
      setInfo({ status, balance });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <section className="dashboard">
      <h2>Dashboard</h2>
      <label>
        Account #<input value={account} placeholder="e.g. 1" onChange={(e) => setAccount(e.target.value)} />
      </label>
      <button onClick={refresh}>Refresh</button>
      {error && <pre className="error">error: {error}</pre>}
      {info && <pre className="result">{JSON.stringify(info, null, 2)}</pre>}
    </section>
  );
}

export function App() {
  const [endpoint, setEndpoint] = useState(
    () => localStorage.getItem(LS_ENDPOINT) || '/rpc'
  );
  const [apiKey, setApiKey] = useState(() => localStorage.getItem(LS_APIKEY) || '');
  const [connection, setConnection] = useState<WalletConnection | null>(() => {
    const ep = localStorage.getItem(LS_ENDPOINT);
    return ep ? new WalletConnection(ep, (localStorage.getItem(LS_APIKEY) || '').trim()) : null;
  });
  const [keyStatus, setKeyStatus] = useState<string | null>(null);

  function connect() {
    const key = apiKey.trim();
    localStorage.setItem(LS_ENDPOINT, endpoint);
    localStorage.setItem(LS_APIKEY, key);
    setConnection(new WalletConnection(endpoint, key));
    setKeyStatus(null);
  }

  function disconnect() {
    localStorage.removeItem(LS_ENDPOINT);
    localStorage.removeItem(LS_APIKEY);
    setConnection(null);
    setKeyStatus(null);
  }

  async function updateKey() {
    if (!connection) return;
    const key = apiKey.trim();
    localStorage.setItem(LS_APIKEY, key);
    setConnection(new WalletConnection(connection.endpoint, key));
    // Probe admin auth with a payload that fails safely at decode time:
    // "-32002 admin API key required" ⇒ bad key; any other error ⇒ key OK.
    setKeyStatus('validando…');
    try {
      await new WalletConnection(connection.endpoint, key).rpc.contractCreate('zz', 'zz', 'zz');
      setKeyStatus('Key válida ✓');
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setKeyStatus(
        /admin API key required|rate limit/.test(msg)
          ? 'Key inválida ✗ — confira o valor colado'
          : 'Key válida ✓'
      );
    }
  }

  return (
    <div className="app">
      <header>
        <h1>AUGECOIN Wallet</h1>
        {!connection ? (
          <div className="connect">
            <input value={endpoint} onChange={(e) => setEndpoint(e.target.value)} placeholder="RPC endpoint" />
            <input value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="Node admin API key (obrigatória p/ contratos)" />
            <button onClick={connect}>
              Connect
            </button>
          </div>
        ) : (
          <div className="connect">
            <span className="connected">● connected</span>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="Node admin API key"
              style={{ maxWidth: 280 }}
            />
            <button onClick={updateKey} title="Aplicar e validar a admin API key">
              Aplicar key
            </button>
            {keyStatus && <span className={keyStatus.includes('✗') ? 'error' : 'connected'}>{keyStatus}</span>}
            <button onClick={disconnect}>Disconnect</button>
          </div>
        )}
      </header>

      {connection ? (
        <WalletContext.Provider value={connection}>
          <Dashboard connection={connection} />
          <SmartContracts />
        </WalletContext.Provider>
      ) : (
        <p className="hint">Conecte-se a um nó para gerenciar smart contracts.</p>
      )}
    </div>
  );
}
