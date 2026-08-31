/**
 * Example: deploy an AUGE20 token and print its contract id.
 *
 * Run with: npx tsx examples/deploy-auge20.ts
 *
 * Env:
 *   AUGECOIN_RPC_URL       node JSON-RPC endpoint (default https://localhost:9443)
 *   AUGECOIN_ADMIN_API_KEY node admin API key (required for contract_create)
 */
import { RpcClient, ContractClient, type TokenInit } from '../src/index';

async function main(): Promise<void> {
  const endpoint = process.env.AUGECOIN_RPC_URL ?? 'https://localhost:9443';
  const apiKey = process.env.AUGECOIN_ADMIN_API_KEY ?? '';

  const name = process.env.TOKEN_NAME ?? 'Auge Coin';
  const symbol = process.env.TOKEN_SYMBOL ?? 'AUG';
  const decimals = Number(process.env.TOKEN_DECIMALS ?? 8);
  const initialSupply = BigInt(process.env.TOKEN_INITIAL_SUPPLY ?? 1_000_000);
  const maxSupply = BigInt(process.env.TOKEN_MAX_SUPPLY ?? 100_000_000);

  const init: TokenInit = {
    name,
    symbol,
    decimals,
    initialSupply,
    maxSupply,
    mintEnabled: true,
    burnEnabled: true,
  };

  const rpc = new RpcClient({ endpoint, apiKey });
  const client = new ContractClient(rpc, apiKey);

  // AUGE20 is a built-in code id (1). For a custom WASM contract, store it
  // first via `client.storeCode(wasmHex)` and use the returned code_id_hex.
  const result = await client.create(1, init);
  if (!result.accepted || !result.contract_id_hex) {
    throw new Error(`contract creation rejected: ${result.error ?? 'unknown error'}`);
  }

  console.log('Token deployed');
  console.log(`  contract_id: ${result.contract_id_hex}`);
  console.log(`  op_hash:     ${result.op_hash_hex ?? '(pending)'}`);
  console.log(`  name:        ${name} (${symbol})`);
  console.log(`  supply:      ${initialSupply} (max ${maxSupply})`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
