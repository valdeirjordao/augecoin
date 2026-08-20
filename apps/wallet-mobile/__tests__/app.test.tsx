import React from 'react';
import { render, fireEvent, waitFor, act } from '@testing-library/react-native';
import App from '../App';

jest.mock('expo-secure-store', () => {
  const store: Record<string, string> = {};
  return {
    setItemAsync: jest.fn(async (key: string, value: string) => { store[key] = value; }),
    getItemAsync: jest.fn(async (key: string) => store[key] ?? null),
    deleteItemAsync: jest.fn(async (key: string) => { delete store[key]; }),
  };
});

jest.mock('react-native-qrcode-svg', () => {
  const React = require('react');
  return (props: any) => React.createElement('View', { testID: 'qr-code-img' });
});

describe('App', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('should render welcome screen by default', () => {
    const { getByText, getByTestId } = render(<App />);
    expect(getByText('AUGECOIN')).toBeTruthy();
    expect(getByText('Carteira Segura')).toBeTruthy();
    expect(getByTestId('create-wallet-btn')).toBeTruthy();
    expect(getByTestId('import-wallet-btn')).toBeTruthy();
  });

  it('should navigate to create screen on "Criar Nova Carteira" press', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('create-wallet-btn'));
    expect(getByText('Suas Palavras de Recuperacao')).toBeTruthy();
  });

  it('should navigate to import screen on "Importar Carteira Existente" press', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));
    expect(getByText('Importar Carteira')).toBeTruthy();
  });

  it('should show mnemonic words on create screen', () => {
    const { getByTestId, getAllByText } = render(<App />);
    fireEvent.press(getByTestId('create-wallet-btn'));
    const words = getAllByText(/^\d+\. \w+$/);
    expect(words.length).toBeGreaterThanOrEqual(12);
  });

  it('should not allow continuing without acknowledging seed backup', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('create-wallet-btn'));

    // Try pressing continue without acknowledging
    fireEvent.press(getByTestId('confirm-seed-btn'));
    // Should still be on create screen (mnemonic words visible)
    expect(getByText('Suas Palavras de Recuperacao')).toBeTruthy();
  });

  it('should allow continuing after acknowledging seed backup', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('create-wallet-btn'));

    fireEvent.press(getByTestId('acknowledge-checkbox'));
    expect(getByTestId('confirm-seed-btn').props.disabled).toBeFalsy();
  });

  it('should navigate to confirm screen after seed acknowledgment', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('create-wallet-btn'));

    fireEvent.press(getByTestId('acknowledge-checkbox'));
    fireEvent.press(getByTestId('confirm-seed-btn'));

    expect(getByText('Confirme sua Seed')).toBeTruthy();
    expect(getByTestId('confirm-word-input')).toBeTruthy();
  });

  it('should show import screen with text input', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    expect(getByTestId('import-seed-input')).toBeTruthy();
    expect(getByTestId('import-btn')).toBeTruthy();
  });

  it('should show error on invalid seed import', () => {
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), 'invalid seed');
    fireEvent.press(getByTestId('import-btn'));

    expect(getByText('Seed invalida. Verifique as 12 palavras.')).toBeTruthy();
  });

  it('should navigate to password screen after valid seed import', () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    expect(getByText('Definir Senha')).toBeTruthy();
  });

  it('should show password validation error when password too short', () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    fireEvent.changeText(getByTestId('password-input'), '1234567');
    fireEvent.changeText(getByTestId('password-confirm-input'), '1234567');
    fireEvent.press(getByTestId('set-password-btn'));

    expect(getByText('A senha deve ter pelo menos 8 caracteres.')).toBeTruthy();
  });

  it('should show error when passwords do not match', () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    fireEvent.changeText(getByTestId('password-input'), '12345678');
    fireEvent.changeText(getByTestId('password-confirm-input'), '87654321');
    fireEvent.press(getByTestId('set-password-btn'));

    expect(getByText('As senhas nao conferem.')).toBeTruthy();
  });

  it('should show home screen with balance, send, and receive tabs', async () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    fireEvent.changeText(getByTestId('password-input'), 'password123');
    fireEvent.changeText(getByTestId('password-confirm-input'), 'password123');
    fireEvent.press(getByTestId('set-password-btn'));

    await waitFor(() => {
      expect(getByText('Carteira AUGECOIN')).toBeTruthy();
    });

    expect(getByTestId('tab-balance')).toBeTruthy();
    expect(getByTestId('tab-send')).toBeTruthy();
    expect(getByTestId('tab-receive')).toBeTruthy();
  });

  it('should display send form when send tab is selected', async () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId, getByText } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    fireEvent.changeText(getByTestId('password-input'), 'password123');
    fireEvent.changeText(getByTestId('password-confirm-input'), 'password123');
    fireEvent.press(getByTestId('set-password-btn'));

    await waitFor(() => {
      expect(getByTestId('tab-send')).toBeTruthy();
    });

    fireEvent.press(getByTestId('tab-send'));

    expect(getByTestId('send-to-input')).toBeTruthy();
    expect(getByTestId('send-amount-input')).toBeTruthy();
    expect(getByTestId('send-btn')).toBeTruthy();
  });

  it('should display receive with QR code when receive tab is selected', async () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    fireEvent.changeText(getByTestId('password-input'), 'password123');
    fireEvent.changeText(getByTestId('password-confirm-input'), 'password123');
    fireEvent.press(getByTestId('set-password-btn'));

    await waitFor(() => {
      expect(getByTestId('tab-receive')).toBeTruthy();
    });

    fireEvent.press(getByTestId('tab-receive'));

    expect(getByTestId('receive-address')).toBeTruthy();
    expect(getByTestId('qr-code-img')).toBeTruthy();
  });

  it('should display balance on home tab', async () => {
    const validMnemonic = 'abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about';
    const { getByTestId } = render(<App />);
    fireEvent.press(getByTestId('import-wallet-btn'));

    fireEvent.changeText(getByTestId('import-seed-input'), validMnemonic);
    fireEvent.press(getByTestId('import-btn'));

    fireEvent.changeText(getByTestId('password-input'), 'password123');
    fireEvent.changeText(getByTestId('password-confirm-input'), 'password123');
    fireEvent.press(getByTestId('set-password-btn'));

    await waitFor(() => {
      expect(getByTestId('balance-display')).toBeTruthy();
    });

    const balanceDisplay = getByTestId('balance-display');
    const text = balanceDisplay.props.children.join('');
    expect(text).toContain('0,00');
    expect(text).toContain('AUGE');
  });
});
