/**
 * Smart Contracts section of the AUGECOIN web wallet (Fase 17.1).
 *
 * Integrates with the connected wallet (endpoint + node admin API key) and the
 * {@link @augecoin/sdk} contract/token clients to: deploy an AUGE20 token,
 * mint, transfer tokens and explore any contract.
 */
import { useState } from 'react';
import { useConnection, type WalletConnection } from '../lib/augecoin';
import { parseTokenAmount } from '../lib/amount';
import type { TokenInit } from '@augecoin/sdk/dist/contracts.js';

type Tab = 'deploy' | 'mint' | 'transfer' | 'explorer';

function Result({ value, error }: { value: unknown; error: string | null }) {
  if (error) return <pre className="error">error: {error}</pre>;
  if (value === null) return null;
  return <pre className="result">{JSON.stringify(value, null, 2)}</pre>;
}

function assertContractId(id: string): string {
  const hex = id.trim();
  if (!/^[0-9a-fA-F]{64}$/.test(hex)) {
    throw new Error('Contract id inválido: informe 64 caracteres hexadecimais.');
  }
  return hex;
}

async function tokenDecimals(connection: WalletConnection, contractId: string): Promise<number> {
  const res = await connection.contracts.tokenInfo(contractId);
  const dec = Number(res?.token?.decimals);
  if (!Number.isInteger(dec) || dec < 0) {
    throw new Error('Não foi possível ler os decimais do contrato — verifique o Contract id.');
  }
  return dec;
}

export function SmartContracts() {
  const connection = useConnection();
  const [tab, setTab] = useState<Tab>('deploy');

  // deploy
  const [name, setName] = useState('Auge Coin');
  const [symbol, setSymbol] = useState('AUG');
  const [decimals, setDecimals] = useState('8');
  const [initialSupply, setInitialSupply] = useState('1000000');
  const [maxSupply, setMaxSupply] = useState('100000000');
  const [mintEnabled, setMintEnabled] = useState(true);
  const [burnEnabled, setBurnEnabled] = useState(true);

  // mint / transfer / explorer
  const [contractId, setContractId] = useState('');
  const [to, setTo] = useState('');
  const [amount, setAmount] = useState('');
  const [address, setAddress] = useState('');

  const [result, setResult] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      setResult(await fn());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setBusy(false);
    }
  }

  function deploy() {
    return run(async () => {
      const dec = Number(decimals);
      if (!Number.isInteger(dec) || dec < 0 || dec > 255) {
        throw new Error(`Decimals inválido: "${decimals}".`);
      }
      const initial = parseTokenAmount(initialSupply, dec);
      const max = parseTokenAmount(maxSupply, dec);
      if (initial > max) {
        throw new Error('Initial supply não pode ser maior que Max supply.');
      }
      const init: TokenInit = {
        name,
        symbol,
        decimals: dec,
        initialSupply: initial,
        maxSupply: max,
        mintEnabled,
        burnEnabled,
      };
      return connection.contracts.create(1, init);
    });
  }

  function mint() {
    return run(async () => {
      const id = assertContractId(contractId);
      const dec = await tokenDecimals(connection, id);
      const dest = Number(to);
      if (!Number.isInteger(dest) || dest < 0) {
        throw new Error(`Conta de destino inválida: "${to}".`);
      }
      const raw = parseTokenAmount(amount, dec);
      return connection.tokens.mint(id, dest, raw);
    });
  }

  function transfer() {
    return run(async () => {
      const id = assertContractId(contractId);
      const dec = await tokenDecimals(connection, id);
      const dest = Number(to);
      if (!Number.isInteger(dest) || dest < 0) {
        throw new Error(`Conta de destino inválida: "${to}".`);
      }
      const raw = parseTokenAmount(amount, dec);
      return connection.tokens.send(id, dest, raw);
    });
  }

  async function explore() {
    return run(async () => {
      const info = await connection.contracts.tokenInfo(contractId);
      const balance =
        address !== ''
          ? await connection.contracts.balanceOf(contractId, Number(address))
          : null;
      return { info, balance };
    });
  }

  return (
    <section className="smart-contracts">
      <h2>Smart Contracts</h2>
      {!connection.apiKey && (
        <pre className="error">
          {'Admin API key não configurada: deploy/mint/transfer vão falhar.\nCole a chave no campo "Node admin API key" do topo e clique "Aplicar key".'}
        </pre>
      )}
      <p className="hint">
        Conectado a <code>{connection.endpoint}</code>. Operações de contrato usam a API key
        de admin do nó (o nó assina com sua própria chave). A chave privada da carteira nunca é
        enviada ao nó.
      </p>

      <div className="tabs">
        <button className={tab === 'deploy' ? 'active' : ''} onClick={() => setTab('deploy')}>
          Deploy AUGE20
        </button>
        <button className={tab === 'mint' ? 'active' : ''} onClick={() => setTab('mint')}>
          Mint
        </button>
        <button className={tab === 'transfer' ? 'active' : ''} onClick={() => setTab('transfer')}>
          Transfer
        </button>
        <button className={tab === 'explorer' ? 'active' : ''} onClick={() => setTab('explorer')}>
          Explorer
        </button>
      </div>

      {tab === 'deploy' && (
        <div className="form">
          <p className="hint">
            Suprimentos em tokens (ex.: 50.000,00 ou 50,000.00) — convertidos
            automaticamente para a menor unidade. Com 8 decimais o limite por campo é
            184.467.440.737,09 tokens (u64).
          </p>
          <label>Name<input value={name} onChange={(e) => setName(e.target.value)} /></label>
          <label>Symbol<input value={symbol} onChange={(e) => setSymbol(e.target.value)} /></label>
          <label>Decimals<input value={decimals} onChange={(e) => setDecimals(e.target.value)} /></label>
          <label>Initial supply<input value={initialSupply} onChange={(e) => setInitialSupply(e.target.value)} /></label>
          <label>Max supply<input value={maxSupply} onChange={(e) => setMaxSupply(e.target.value)} /></label>
          <label>
            <input type="checkbox" checked={mintEnabled} onChange={(e) => setMintEnabled(e.target.checked)} /> Mint enabled
          </label>
          <label>
            <input type="checkbox" checked={burnEnabled} onChange={(e) => setBurnEnabled(e.target.checked)} /> Burn enabled
          </label>
          <button disabled={busy} onClick={deploy}>
            {busy ? '...' : 'Deploy token (code id 1)'}
          </button>
        </div>
      )}

      {tab === 'mint' && (
        <div className="form">
          <label>Contract id (hex)<input value={contractId} onChange={(e) => setContractId(e.target.value)} /></label>
          <label>To (account)<input value={to} onChange={(e) => setTo(e.target.value)} /></label>
          <label>Amount (tokens)<input value={amount} onChange={(e) => setAmount(e.target.value)} /></label>
          <button disabled={busy} onClick={mint}>
            {busy ? '...' : 'Mint'}
          </button>
        </div>
      )}

      {tab === 'transfer' && (
        <div className="form">
          <label>Contract id (hex)<input value={contractId} onChange={(e) => setContractId(e.target.value)} /></label>
          <label>To (account)<input value={to} onChange={(e) => setTo(e.target.value)} /></label>
          <label>Amount (tokens)<input value={amount} onChange={(e) => setAmount(e.target.value)} /></label>
          <button disabled={busy} onClick={transfer}>
            {busy ? '...' : 'Transfer'}
          </button>
        </div>
      )}

      {tab === 'explorer' && (
        <div className="form">
          <label>Contract id (hex)<input value={contractId} onChange={(e) => setContractId(e.target.value)} /></label>
          <label>Owner account (optional)<input value={address} onChange={(e) => setAddress(e.target.value)} /></label>
          <button disabled={busy} onClick={explore}>
            {busy ? '...' : 'Lookup'}
          </button>
        </div>
      )}

      <Result value={result} error={error} />
    </section>
  );
}
