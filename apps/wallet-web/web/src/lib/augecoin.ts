/**
 * Wallet connection wiring for the AUGECOIN web wallet (Fase 17).
 *
 * Bridges the wallet UI to the {@link @augecoin/sdk} RPC + contract clients.
 * Contract operations are submitted with the node admin API key (the node
 * signs with its own keypair), so the user's private key is never sent to the
 * node — it stays encrypted in the local keystore.
 */
import { createContext, useContext } from 'react';
import { RpcClient } from '@augecoin/sdk/dist/rpc.js';
import { ContractClient, TokenClient } from '@augecoin/sdk/dist/contracts.js';

export class WalletConnection {
  readonly rpc: RpcClient;
  readonly contracts: ContractClient;
  readonly tokens: TokenClient;

  constructor(
    public readonly endpoint: string,
    public readonly apiKey: string = ''
  ) {
    this.rpc = new RpcClient({ endpoint, apiKey });
    this.contracts = new ContractClient(this.rpc, apiKey);
    this.tokens = new TokenClient(this.rpc, apiKey);
  }
}

export const WalletContext = createContext<WalletConnection | null>(null);

export function useConnection(): WalletConnection {
  const connection = useContext(WalletContext);
  if (!connection) throw new Error('wallet not connected');
  return connection;
}
