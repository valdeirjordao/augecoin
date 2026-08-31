import { describe, it, expect } from 'vitest';
import { encodeTokenInit, encodeTokenCall } from '../src/contracts';
import { bytesToHex, codeIdToBeHex, addressToBeHex, u64ToBeHex } from '../src/hex';

describe('codec parity with Rust augecoin-contracts', () => {
  it('encodes TokenCall::Transfer (to=7, amount=500)', () => {
    const hex = bytesToHex(encodeTokenCall({ kind: 'transfer', to: 7, amount: 500 }));
    expect(hex).toBe('00000000000000000700000000000001f4');
  });

  it('encodes TokenCall::Mint (to=7, amount=1000)', () => {
    const hex = bytesToHex(encodeTokenCall({ kind: 'mint', to: 7, amount: 1000 }));
    expect(hex).toBe('01000000000000000700000000000003e8');
  });

  it('encodes TokenCall::Burn (amount=250)', () => {
    const hex = bytesToHex(encodeTokenCall({ kind: 'burn', amount: 250 }));
    expect(hex).toBe('0200000000000000fa');
  });

  it('encodes TokenInit (AUGE20: Auge/AUG, 8 decimals, 1e6/2e6 supply)', () => {
    const hex = bytesToHex(
      encodeTokenInit({
        name: 'Auge',
        symbol: 'AUG',
        decimals: 8,
        initialSupply: 1_000_000,
        maxSupply: 2_000_000,
        mintEnabled: true,
        burnEnabled: true,
      })
    );
    expect(hex).toBe('08010100000000000f424000000000001e8480000000044175676500000003415547');
  });
});

describe('hex / big-endian helpers', () => {
  it('codeIdToBeHex(1) -> 0000000000000001', () => {
    expect(codeIdToBeHex(1)).toBe('0000000000000001');
  });

  it('addressToBeHex(42) -> 000000000000002a', () => {
    expect(addressToBeHex(42)).toBe('000000000000002a');
  });

  it('u64ToBeHex roundtrips', () => {
    expect(u64ToBeHex(0x0102030405060708n)).toBe('0102030405060708');
  });
});
