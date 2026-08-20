import React, { useState, useEffect, useCallback } from 'react';
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  ScrollView,
  StyleSheet,
  ActivityIndicator,
  SafeAreaView,
} from 'react-native';
import QRCode from 'react-native-qrcode-svg';
import { createWallet, importWallet, isValidMnemonic, getWordCount } from './src/lib/wallet';
import { saveWallet, loadWallet, deleteWallet, walletExists } from './src/lib/keystore';

type Screen = 'welcome' | 'create' | 'confirm' | 'import' | 'password' | 'home';

export default function App() {
  const [screen, setScreen] = useState<Screen>('welcome');
  const [mnemonic, setMnemonic] = useState('');
  const [seed, setSeed] = useState<Uint8Array | null>(null);
  const [password, setPassword] = useState('');
  const [accounts, setAccounts] = useState<Array<{ num: number; balance: string }>>([
    { num: 0, balance: '0,00' },
  ]);

  useEffect(() => {
    if (seed && password) {
      saveWallet(seed, password)
        .then(() => setScreen('home'))
        .catch(() => {});
    }
  }, [seed, password]);

  return (
    <SafeAreaView style={styles.container}>
      {screen === 'welcome' && (
        <WelcomeScreen
          onCreate={() => {
            const w = createWallet();
            setMnemonic(w.mnemonic);
            setScreen('create');
          }}
          onImport={() => setScreen('import')}
        />
      )}
      {screen === 'create' && (
        <CreateScreen
          mnemonic={mnemonic}
          onContinue={() => setScreen('confirm')}
        />
      )}
      {screen === 'confirm' && (
        <ConfirmScreen
          mnemonic={mnemonic}
          onConfirmed={() => setScreen('password')}
        />
      )}
      {screen === 'import' && (
        <ImportScreen
          onImported={(m) => {
            const w = importWallet(m);
            setMnemonic(w.mnemonic);
            setScreen('password');
          }}
        />
      )}
      {screen === 'password' && (
        <PasswordScreen
          onPasswordSet={(pw) => {
            setPassword(pw);
            setSeed(importWallet(mnemonic).seed);
          }}
        />
      )}
      {screen === 'home' && <HomeScreen accounts={accounts} />}
    </SafeAreaView>
  );
}

function WelcomeScreen({ onCreate, onImport }: { onCreate: () => void; onImport: () => void }) {
  return (
    <View style={styles.screen}>
      <Text style={styles.title}>AUGECOIN</Text>
      <Text style={styles.subtitle}>Carteira Segura</Text>
      <View style={styles.actions}>
        <TouchableOpacity
          style={styles.primaryButton}
          onPress={onCreate}
          testID="create-wallet-btn"
        >
          <Text style={styles.primaryButtonText}>Criar Nova Carteira</Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={styles.secondaryButton}
          onPress={onImport}
          testID="import-wallet-btn"
        >
          <Text style={styles.secondaryButtonText}>Importar Carteira Existente</Text>
        </TouchableOpacity>
      </View>
    </View>
  );
}

function CreateScreen({ mnemonic, onContinue }: { mnemonic: string; onContinue: () => void }) {
  const [acknowledged, setAcknowledged] = useState(false);

  return (
    <ScrollView style={styles.screen}>
      <Text style={styles.h2}>Suas Palavras de Recuperacao</Text>
      <View style={styles.warningBox}>
        <Text style={styles.warningText}>
          Guarde estas 12 palavras em local seguro. Elas serao exibidas APENAS UMA VEZ.
          Quem tiver acesso a elas controla sua carteira.
        </Text>
      </View>
      <View style={styles.mnemonicBox}>
        {mnemonic.split(' ').map((word, i) => (
          <Text key={i} style={styles.mnemonicWord}>
            {i + 1}. {word}
          </Text>
        ))}
      </View>
      <TouchableOpacity
        style={styles.checkboxRow}
        onPress={() => setAcknowledged(!acknowledged)}
        testID="acknowledge-checkbox"
      >
        <View style={[styles.checkbox, acknowledged && styles.checkboxChecked]} />
        <Text style={styles.checkboxLabel}>
          Eu anotei minhas palavras de recuperacao
        </Text>
      </TouchableOpacity>
      <TouchableOpacity
        style={[styles.primaryButton, !acknowledged && styles.buttonDisabled]}
        onPress={onContinue}
        disabled={!acknowledged}
        testID="confirm-seed-btn"
      >
        <Text style={styles.primaryButtonText}>Continuar</Text>
      </TouchableOpacity>
    </ScrollView>
  );
}

function ConfirmScreen({ mnemonic, onConfirmed }: { mnemonic: string; onConfirmed: () => void }) {
  const [input, setInput] = useState('');
  const [error, setError] = useState('');
  const wordCount = getWordCount(mnemonic);
  const words = mnemonic.split(' ');
  const randomIndex = Math.floor(Math.random() * wordCount);
  const expectedWord = words[randomIndex];

  const handleVerify = () => {
    if (input.trim().toLowerCase() === expectedWord.toLowerCase()) {
      onConfirmed();
    } else {
      setError(`Palavra incorreta. A palavra #${randomIndex + 1} nao confere.`);
    }
  };

  return (
    <View style={styles.screen}>
      <Text style={styles.h2}>Confirme sua Seed</Text>
      <Text style={styles.bodyText}>
        Digite a palavra <Text style={styles.bold}>#{randomIndex + 1}</Text> da sua seed:
      </Text>
      <TextInput
        style={styles.textInput}
        value={input}
        onChangeText={(t) => { setInput(t); setError(''); }}
        placeholder={`Palavra #${randomIndex + 1}`}
        placeholderTextColor="#555"
        testID="confirm-word-input"
      />
      {error ? <Text style={styles.errorText}>{error}</Text> : null}
      <TouchableOpacity
        style={styles.primaryButton}
        onPress={handleVerify}
        testID="verify-seed-btn"
      >
        <Text style={styles.primaryButtonText}>Verificar</Text>
      </TouchableOpacity>
    </View>
  );
}

function ImportScreen({ onImported }: { onImported: (m: string) => void }) {
  const [input, setInput] = useState('');
  const [error, setError] = useState('');

  const handleImport = () => {
    if (!isValidMnemonic(input)) {
      setError('Seed invalida. Verifique as 12 palavras.');
      return;
    }
    onImported(input.trim());
  };

  return (
    <View style={styles.screen}>
      <Text style={styles.h2}>Importar Carteira</Text>
      <Text style={styles.bodyText}>Digite suas 12 palavras de recuperacao:</Text>
      <TextInput
        style={[styles.textInput, styles.textArea]}
        value={input}
        onChangeText={(t) => { setInput(t); setError(''); }}
        placeholder="abandon abandon ... about"
        placeholderTextColor="#555"
        multiline
        numberOfLines={3}
        testID="import-seed-input"
      />
      {error ? <Text style={styles.errorText}>{error}</Text> : null}
      <TouchableOpacity
        style={styles.primaryButton}
        onPress={handleImport}
        testID="import-btn"
      >
        <Text style={styles.primaryButtonText}>Importar</Text>
      </TouchableOpacity>
    </View>
  );
}

function PasswordScreen({ onPasswordSet }: { onPasswordSet: (pw: string) => void }) {
  const [pw, setPw] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState('');

  const handleSet = () => {
    if (pw.length < 8) {
      setError('A senha deve ter pelo menos 8 caracteres.');
      return;
    }
    if (pw !== confirm) {
      setError('As senhas nao conferem.');
      return;
    }
    onPasswordSet(pw);
  };

  return (
    <View style={styles.screen}>
      <Text style={styles.h2}>Definir Senha</Text>
      <Text style={styles.bodyText}>
        Esta senha protege sua carteira neste dispositivo.
      </Text>
      <TextInput
        style={styles.textInput}
        value={pw}
        onChangeText={(t) => { setPw(t); setError(''); }}
        placeholder="Senha (min. 8 caracteres)"
        placeholderTextColor="#555"
        secureTextEntry
        testID="password-input"
      />
      <TextInput
        style={styles.textInput}
        value={confirm}
        onChangeText={(t) => { setConfirm(t); setError(''); }}
        placeholder="Confirmar senha"
        placeholderTextColor="#555"
        secureTextEntry
        testID="password-confirm-input"
      />
      {error ? <Text style={styles.errorText}>{error}</Text> : null}
      <TouchableOpacity
        style={styles.primaryButton}
        onPress={handleSet}
        testID="set-password-btn"
      >
        <Text style={styles.primaryButtonText}>Proteger Carteira</Text>
      </TouchableOpacity>
    </View>
  );
}

function HomeScreen({ accounts }: { accounts: Array<{ num: number; balance: string }> }) {
  const [activeTab, setActiveTab] = useState<'balance' | 'send' | 'receive'>('balance');
  const [toAccount, setToAccount] = useState('');
  const [amount, setAmount] = useState('');

  return (
    <View style={styles.screen}>
      <Text style={styles.h2}>Carteira AUGECOIN</Text>

      <View style={styles.balanceCard}>
        <Text style={styles.balanceLabel}>Saldo Total</Text>
        <Text style={styles.balanceValue} testID="balance-display">
          {accounts[0]?.balance ?? '0,00'} AUGE
        </Text>
      </View>

      <View style={styles.tabs}>
        <TouchableOpacity
          style={[styles.tab, activeTab === 'balance' && styles.tabActive]}
          onPress={() => setActiveTab('balance')}
          testID="tab-balance"
        >
          <Text style={[styles.tabText, activeTab === 'balance' && styles.tabTextActive]}>
            Saldo
          </Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={[styles.tab, activeTab === 'send' && styles.tabActive]}
          onPress={() => setActiveTab('send')}
          testID="tab-send"
        >
          <Text style={[styles.tabText, activeTab === 'send' && styles.tabTextActive]}>
            Enviar
          </Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={[styles.tab, activeTab === 'receive' && styles.tabActive]}
          onPress={() => setActiveTab('receive')}
          testID="tab-receive"
        >
          <Text style={[styles.tabText, activeTab === 'receive' && styles.tabTextActive]}>
            Receber
          </Text>
        </TouchableOpacity>
      </View>

      {activeTab === 'balance' && (
        <View style={styles.tabContent}>
          <Text style={styles.h3}>Contas</Text>
          {accounts.map((acc) => (
            <View key={acc.num} style={styles.accountRow} testID={`account-${acc.num}`}>
              <Text style={styles.bodyText}>Conta #{acc.num}</Text>
              <Text style={styles.bodyText}>{acc.balance} AUGE</Text>
            </View>
          ))}
        </View>
      )}

      {activeTab === 'send' && (
        <View style={styles.tabContent}>
          <Text style={styles.h3}>Enviar AUGE</Text>
          <TextInput
            style={styles.textInput}
            value={toAccount}
            onChangeText={setToAccount}
            placeholder="Conta destino (numero)"
            placeholderTextColor="#555"
            keyboardType="numeric"
            testID="send-to-input"
          />
          <TextInput
            style={styles.textInput}
            value={amount}
            onChangeText={setAmount}
            placeholder="Quantidade em AUGE"
            placeholderTextColor="#555"
            keyboardType="decimal-pad"
            testID="send-amount-input"
          />
          <TouchableOpacity style={styles.primaryButton} testID="send-btn">
            <Text style={styles.primaryButtonText}>Enviar</Text>
          </TouchableOpacity>
        </View>
      )}

      {activeTab === 'receive' && (
        <View style={styles.tabContent}>
          <Text style={styles.h3}>Receber AUGE</Text>
          <Text style={styles.bodyText}>
            Seu endereco:{' '}
            <Text style={styles.codeText} testID="receive-address">auge1...</Text>
          </Text>
          <View style={styles.qrContainer} testID="qr-code-container">
            <QRCode
              value="auge1placeholder000000000000000000000000"
              size={200}
            />
          </View>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#0f0f1a',
  },
  screen: {
    flex: 1,
    padding: 20,
    paddingTop: 10,
  },
  title: {
    fontSize: 28,
    fontWeight: '700',
    color: '#7c5cfc',
    textAlign: 'center',
    marginTop: 20,
  },
  subtitle: {
    fontSize: 16,
    color: '#888',
    textAlign: 'center',
    marginTop: 8,
    marginBottom: 30,
  },
  h2: {
    fontSize: 22,
    fontWeight: '700',
    color: '#a78bfa',
    marginBottom: 16,
  },
  h3: {
    fontSize: 18,
    fontWeight: '600',
    color: '#a78bfa',
    marginBottom: 12,
  },
  bodyText: {
    fontSize: 16,
    color: '#ccc',
    lineHeight: 24,
  },
  bold: {
    fontWeight: '700',
  },
  actions: {
    marginTop: 10,
    gap: 12,
  },
  primaryButton: {
    backgroundColor: '#7c5cfc',
    paddingVertical: 14,
    paddingHorizontal: 24,
    borderRadius: 12,
    alignItems: 'center',
    marginTop: 16,
  },
  primaryButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  secondaryButton: {
    backgroundColor: '#2a2a3d',
    paddingVertical: 14,
    paddingHorizontal: 24,
    borderRadius: 12,
    alignItems: 'center',
  },
  secondaryButtonText: {
    color: '#a78bfa',
    fontSize: 16,
    fontWeight: '600',
  },
  buttonDisabled: {
    opacity: 0.4,
  },
  warningBox: {
    backgroundColor: '#332200',
    borderWidth: 1,
    borderColor: '#664400',
    padding: 12,
    borderRadius: 8,
    marginBottom: 16,
  },
  warningText: {
    color: '#ffa500',
    fontSize: 14,
    lineHeight: 20,
  },
  mnemonicBox: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
    backgroundColor: '#1a1a2e',
    padding: 16,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: '#2a2a3d',
    marginBottom: 16,
  },
  mnemonicWord: {
    fontSize: 14,
    color: '#a78bfa',
    fontFamily: 'monospace',
    width: '30%',
  },
  checkboxRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginBottom: 8,
  },
  checkbox: {
    width: 22,
    height: 22,
    borderRadius: 4,
    borderWidth: 2,
    borderColor: '#555',
  },
  checkboxChecked: {
    backgroundColor: '#7c5cfc',
    borderColor: '#7c5cfc',
  },
  checkboxLabel: {
    fontSize: 14,
    color: '#ccc',
    flex: 1,
  },
  textInput: {
    backgroundColor: '#1a1a2e',
    borderWidth: 1,
    borderColor: '#2a2a3d',
    borderRadius: 8,
    padding: 12,
    color: '#e0e0e0',
    fontSize: 16,
    marginBottom: 12,
  },
  textArea: {
    height: 80,
    textAlignVertical: 'top',
  },
  errorText: {
    color: '#ff4444',
    fontSize: 14,
    marginBottom: 8,
  },
  balanceCard: {
    backgroundColor: '#7c5cfc',
    padding: 24,
    borderRadius: 16,
    alignItems: 'center',
    marginBottom: 16,
  },
  balanceLabel: {
    fontSize: 14,
    color: 'rgba(255,255,255,0.8)',
  },
  balanceValue: {
    fontSize: 32,
    fontWeight: '700',
    color: '#fff',
    marginTop: 8,
  },
  tabs: {
    flexDirection: 'row',
    backgroundColor: '#1a1a2e',
    borderRadius: 12,
    padding: 4,
    marginBottom: 16,
  },
  tab: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 8,
    alignItems: 'center',
  },
  tabActive: {
    backgroundColor: '#7c5cfc',
  },
  tabText: {
    fontSize: 14,
    color: '#888',
    fontWeight: '600',
  },
  tabTextActive: {
    color: '#fff',
  },
  tabContent: {
    flex: 1,
    gap: 10,
  },
  accountRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    padding: 12,
    backgroundColor: '#1a1a2e',
    borderRadius: 8,
  },
  codeText: {
    fontFamily: 'monospace',
    backgroundColor: '#1a1a2e',
    color: '#7c5cfc',
    fontSize: 14,
  },
  qrContainer: {
    alignItems: 'center',
    padding: 16,
    backgroundColor: '#fff',
    borderRadius: 12,
    marginTop: 8,
  },
});
