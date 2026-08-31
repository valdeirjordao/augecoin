/**
 * Example: mint AUGE20 tokens to an account.
 *
 * Run with: npx tsx examples/mint.ts
 *
 * Env:
 *   AUGECOIN_RPC_URL       node JSON-RPC endpoint
 *   AUGECOIN_ADMIN_API_KEY node admin API key
 *   CONTRACT_ID            contract id hex (32 bytes)
 *   TO                     recipient account number
 *   AMOUNT                 whole-token amount to mint
 */
import { RpcClient, TokenClient } from '../src/index';

async function main(): Promise<void> {
  const endpoint = process.env.AUGECOIN_RPC_URL ?? 'https://localhost:9443';
  const apiKey = process.env.AUGECOIN_ADMIN_API_KEY ?? '';
  const contractId = process.env.CONTRACT_ID;
  const to = Number(process.env.TO);
  const amount = BigInt(process.env.AMOUNT ?? 0);

  if (!contractId) throw new Error('CONTRACT_ID env (32-byte hex) is required');
  if (!Number.isFinite(to)) throw new Error('TO env (account number) is required');
  if (amount <= 0n) throw new Error('AMOUNT env must be > 0');

  const rpc = new RpcClient({ endpoint, apiKey });
  const token = new TokenClient(rpc, apiKey);

  const result = await token.mint(contractId, to, amount);
  if (!result.success) {
    throw new Error(`mint rejected: ${result.error ?? 'unknown error'}`);
  }

  console.log('Mint submitted');
  console.log(`  contract_id: ${contractId}`);
  console.log(`  to:          ${to}`);
  console.log(`  amount:      ${amount}`);
  console.log(`  gas_used:    ${result.gas_used ?? '(unknown)'}`);
  console.log(`  op_hash:     ${result.op_hash_hex ?? '(pending)'}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
