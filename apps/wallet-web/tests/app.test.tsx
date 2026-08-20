import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';
import App from '../src/App';
import type { AccountInfo, PlatformUser } from '../src/types';

// ── Service mocks (keep the blockchain/consensus out of the UI tests) ──
vi.mock('../src/services/platform', () => ({
  registerMember: vi.fn(),
  loginMember: vi.fn(),
  restoreSession: vi.fn().mockResolvedValue(null),
  logoutMember: vi.fn().mockResolvedValue(undefined),
  updateProfile: vi.fn(),
  updatePreferences: vi.fn(),
  directory: vi.fn().mockResolvedValue([]),
  displayNameByKey: vi.fn().mockResolvedValue('Vendedor'),
}));

vi.mock('../src/services/walletRegistry', () => ({
  listLinkedWallets: vi.fn(),
  linkWallet: vi.fn(),
  unlinkWallet: vi.fn(),
}));

vi.mock('../src/services/rpc', () => ({
  getAccount: vi.fn(),
  listAccountsForSale: vi.fn(),
  listPendingGifts: vi.fn(),
  getNodeStatus: vi.fn(),
  resolveName: vi.fn(),
  isNameAvailable: vi.fn(),
  findAccounts: vi.fn(),
  listValidatorInventory: vi.fn(),
  getNodeStatusFallback: vi.fn(),
}));

vi.mock('../src/services/operations', () => ({
  submitBuy: vi.fn(),
  submitAcceptGift: vi.fn(),
  submitSell: vi.fn(),
  submitCancelSale: vi.fn(),
  submitGift: vi.fn(),
  submitChangeKey: vi.fn(),
  submitChangeAccountInfo: vi.fn(),
  submitTransfer: vi.fn(),
}));

import * as platform from '../src/services/platform';
import * as registry from '../src/services/walletRegistry';
import * as rpc from '../src/services/rpc';
import * as ops from '../src/services/operations';

const KEY = 'aa'.repeat(32);
const KEY2 = 'bb'.repeat(32);
const KEY3 = 'cc'.repeat(32);

function user(overrides: Partial<PlatformUser> = {}): PlatformUser {
  return {
    id: 'u1',
    email: 'alice@example.com',
    display_name: 'Alice',
    public_key_hex: KEY,
    created_at: '2026-08-16T00:00:00.000Z',
    last_login: null,
    ...overrides,
  };
}

function account(number: number, overrides: Partial<AccountInfo> = {}): AccountInfo {
  return {
    account_number: number,
    balance: 0,
    n_operation: 0,
    name: null,
    account_type: 0,
    account_data_hex: '',
    account_seal_hex: '',
    state: 'Normal',
    updated_on_block_passive_mode: 0,
    updated_on_block_active_mode: 0,
    locked_until_block: 0,
    price: 0,
    account_to_pay: 0,
    account_key_ed_hex: KEY,
    ...overrides,
  };
}

const mocks = {
  platform: vi.mocked(platform),
  registry: vi.mocked(registry),
  rpc: vi.mocked(rpc),
  ops: vi.mocked(ops),
};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.platform.restoreSession.mockResolvedValue(null);
  mocks.platform.logoutMember.mockResolvedValue(undefined);
  mocks.platform.directory.mockResolvedValue([]);
  mocks.platform.displayNameByKey.mockResolvedValue('Vendedor');
  mocks.registry.listLinkedWallets.mockResolvedValue([]);
  mocks.registry.linkWallet.mockResolvedValue({
    user_id: 'u1',
    account_number: 0,
    linked_at: '2026-08-16T00:00:00.000Z',
  });
  mocks.rpc.getNodeStatus.mockResolvedValue({ chain_id: 2 } as never);
});

describe('Wallet Web — separação Conta da Plataforma / Carteira Blockchain', () => {
  it('cadastro cria apenas a Conta da Plataforma (sem AUGEID)', async () => {
    mocks.platform.registerMember.mockResolvedValue({ user: user(), mnemonic: 'mnemonic' });

    render(<App router="memory" />);

    fireEvent.click(screen.getByTestId('tab-register'));
    fireEvent.change(screen.getByTestId('register-name'), { target: { value: 'Alice' } });
    fireEvent.change(screen.getByTestId('auth-email'), { target: { value: 'alice@example.com' } });
    fireEvent.change(screen.getByTestId('auth-password'), { target: { value: 'password123' } });
    fireEvent.click(screen.getByTestId('auth-submit'));

    await waitFor(() => {
      expect(screen.getByTestId('platform-account-card')).toBeInTheDocument();
    });

    // Carteira Blockchain ainda não ativada — nenhum AUGEID foi criado.
    expect(screen.getByTestId('empty-wallet-card')).toBeInTheDocument();
    expect(screen.queryByTestId('blockchain-wallet-card-100')).not.toBeInTheDocument();
    expect(mocks.registry.linkWallet).not.toHaveBeenCalled();
    expect(mocks.rpc.getAccount).not.toHaveBeenCalled();
  });

  it('login sem AUGEID mostra EmptyWalletCard', async () => {
    mocks.platform.loginMember.mockResolvedValue({ user: user(), mnemonic: 'mnemonic' });

    render(<App router="memory" />);

    fireEvent.change(screen.getByTestId('auth-email'), { target: { value: 'alice@example.com' } });
    fireEvent.change(screen.getByTestId('auth-password'), { target: { value: 'password123' } });
    fireEvent.click(screen.getByTestId('auth-submit'));

    await waitFor(() => {
      expect(screen.getByTestId('empty-wallet-card')).toBeInTheDocument();
    });
    expect(screen.getByTestId('platform-account-card')).toBeInTheDocument();
  });

  it('menus financeiros ficam ocultos sem AUGEID', async () => {
    mocks.platform.loginMember.mockResolvedValue({ user: user(), mnemonic: 'mnemonic' });

    render(<App router="memory" />);

    fireEvent.change(screen.getByTestId('auth-email'), { target: { value: 'alice@example.com' } });
    fireEvent.change(screen.getByTestId('auth-password'), { target: { value: 'password123' } });
    fireEvent.click(screen.getByTestId('auth-submit'));

    await waitFor(() => {
      expect(screen.getByTestId('nav-locked')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('nav-send')).not.toBeInTheDocument();
    expect(screen.queryByTestId('nav-receive')).not.toBeInTheDocument();
    expect(screen.queryByTestId('nav-history')).not.toBeInTheDocument();
    expect(screen.queryByTestId('nav-my-wallets')).not.toBeInTheDocument();
  });

  it('compra vincula o AUGEID automaticamente', async () => {
    mocks.platform.loginMember.mockResolvedValue({ user: user(), mnemonic: 'mnemonic' });
    mocks.registry.listLinkedWallets.mockResolvedValue([
      { user_id: 'u1', account_number: 100, linked_at: 'x' },
    ]);
    mocks.rpc.getAccount.mockImplementation(async (n: number) => {
      if (n === 100) return account(100, { balance: 100_000_000, state: 'Normal' });
      if (n === 200)
        return account(200, {
          state: 'ForSale',
          price: 50_000_000,
          account_to_pay: 100,
          account_key_ed_hex: KEY3,
        });
      return account(n);
    });
    mocks.rpc.listAccountsForSale.mockResolvedValue({
      entries: [
        {
          account_number: 200,
          price: 50_000_000,
          seller_public_key_hex: KEY2,
          state: 'ForSale',
          listed_at_block: 5,
        },
      ],
    });
    mocks.ops.submitBuy.mockResolvedValue({ accepted: true, op_hash_hex: '0x1', error: null });
    mocks.registry.linkWallet.mockResolvedValue({ user_id: 'u1', account_number: 200, linked_at: 'x' });

    render(<App router="memory" />);

    fireEvent.change(screen.getByTestId('auth-email'), { target: { value: 'alice@example.com' } });
    fireEvent.change(screen.getByTestId('auth-password'), { target: { value: 'password123' } });
    fireEvent.click(screen.getByTestId('auth-submit'));

    await waitFor(() => {
      expect(screen.getByTestId('blockchain-wallet-card-100')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('Marketplace'));

    await waitFor(() => {
      expect(screen.getByTestId('marketplace-card-200')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('buy-200'));
    await waitFor(() => {
      expect(screen.getByTestId('confirm-buy')).toBeInTheDocument();
    });
    fireEvent.click(screen.getByTestId('confirm-buy'));

    await waitFor(() => {
      expect(mocks.registry.linkWallet).toHaveBeenCalledWith(200);
    });
  });

  it('doação vincula o AUGEID automaticamente ao aceitar', async () => {
    mocks.platform.loginMember.mockResolvedValue({ user: user(), mnemonic: 'mnemonic' });
    mocks.rpc.listPendingGifts.mockResolvedValue({
      entries: [
        {
          account_number: 300,
          recipient_public_key_hex: KEY,
          from_public_key_hex: KEY2,
          gifted_at_block: 3,
          name: 'Presente',
        },
      ],
    });
    mocks.rpc.getAccount.mockImplementation(async (n: number) =>
      n === 300 ? account(300, { state: 'GiftPending', account_key_ed_hex: KEY2 }) : account(n),
    );
    mocks.ops.submitAcceptGift.mockResolvedValue({ accepted: true, op_hash_hex: '0x2', error: null });
    mocks.registry.linkWallet.mockResolvedValue({ user_id: 'u1', account_number: 300, linked_at: 'x' });

    render(<App router="memory" />);

    fireEvent.change(screen.getByTestId('auth-email'), { target: { value: 'alice@example.com' } });
    fireEvent.change(screen.getByTestId('auth-password'), { target: { value: 'password123' } });
    fireEvent.click(screen.getByTestId('auth-submit'));

    await waitFor(() => {
      expect(screen.getByTestId('empty-wallet-card')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('cta-receive-gift'));

    await waitFor(() => {
      expect(screen.getByTestId('gift-card-300')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('accept-300'));

    await waitFor(() => {
      expect(mocks.registry.linkWallet).toHaveBeenCalledWith(300);
    });
  });

  it('múltiplos AUGEIDs aparecem corretamente', async () => {
    mocks.platform.loginMember.mockResolvedValue({ user: user(), mnemonic: 'mnemonic' });
    mocks.registry.listLinkedWallets.mockResolvedValue([
      { user_id: 'u1', account_number: 154650, linked_at: 'x' },
      { user_id: 'u1', account_number: 154812, linked_at: 'x' },
    ]);
    mocks.rpc.getAccount.mockImplementation(async (n: number) => {
      if (n === 154650) return account(154650, { name: 'CarlosPay', balance: 12_000_000 });
      if (n === 154812) return account(154812, { name: 'LojaXPTO', state: 'ForSale' });
      return account(n);
    });

    render(<App router="memory" />);

    fireEvent.change(screen.getByTestId('auth-email'), { target: { value: 'alice@example.com' } });
    fireEvent.change(screen.getByTestId('auth-password'), { target: { value: 'password123' } });
    fireEvent.click(screen.getByTestId('auth-submit'));

    await waitFor(() => {
      expect(screen.getByTestId('blockchain-wallet-card-154650')).toBeInTheDocument();
    });
    expect(screen.getByTestId('blockchain-wallet-card-154812')).toBeInTheDocument();

    // Inventário (Minhas Carteiras) lista ambos.
    fireEvent.click(screen.getByTestId('nav-my-wallets'));
    await waitFor(() => {
      expect(screen.getByTestId('wallet-row-154650')).toBeInTheDocument();
    });
    expect(screen.getByTestId('wallet-row-154812')).toBeInTheDocument();
  });
});
