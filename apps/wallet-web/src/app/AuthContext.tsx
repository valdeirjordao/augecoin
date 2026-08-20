import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import { CONFIG } from '../services/config';
import * as platform from '../services/platform';
import * as rpc from '../services/rpc';
import type { Session } from '../services/operations';
import type { PlatformUser, UserPreferences } from '../types';

interface AuthState {
  user: PlatformUser | null;
  preferences: UserPreferences | null;
  mnemonic: string | null;
  chainId: bigint;
  loading: boolean;
  error: string | null;
  session: Session | null;
}

interface AuthContextValue extends AuthState {
  register: (args: { name: string; email: string; password: string }) => Promise<void>;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  updateProfile: (displayName: string) => Promise<void>;
  updatePreferences: (prefs: Partial<UserPreferences>) => Promise<void>;
  clearError: () => void;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<PlatformUser | null>(null);
  const [preferences, setPreferences] = useState<UserPreferences | null>(null);
  const [mnemonic, setMnemonic] = useState<string | null>(null);
  const [chainId, setChainId] = useState<bigint>(CONFIG.DEFAULT_CHAIN_ID);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    rpc
      .getNodeStatus()
      .then((s) => setChainId(BigInt(s.chain_id)))
      .catch(() => {
        /* fall back to default chain id */
      });
  }, []);

  // Restore the platform session (identity + preferences) from the cookie.
  // The mnemonic is intentionally NOT restored — it stays client-only and must
  // be re-unlocked with the password to sign on-chain operations.
  useEffect(() => {
    let active = true;
    platform
      .restoreSession()
      .then((session) => {
        if (active && session) {
          setUser(session.user);
          setPreferences(session.preferences);
        }
      })
      .catch(() => {
        /* not logged in */
      });
    return () => {
      active = false;
    };
  }, []);

  const register = useCallback(
    async (args: { name: string; email: string; password: string }) => {
      setLoading(true);
      setError(null);
      try {
        const { user: u, mnemonic: m } = await platform.registerMember(args);
        setUser(u);
        setMnemonic(m);
        setPreferences({ theme: 'dark', language: 'pt-BR', notifications: true });
      } catch (e) {
        setError(e instanceof Error ? e.message : 'Erro ao cadastrar.');
        throw e;
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const login = useCallback(async (email: string, password: string) => {
    setLoading(true);
    setError(null);
    try {
      const { user: u, mnemonic: m } = await platform.loginMember(email, password);
      setUser(u);
      setMnemonic(m);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Erro ao entrar.');
      throw e;
    } finally {
      setLoading(false);
    }
  }, []);

  const logout = useCallback(async () => {
    await platform.logoutMember();
    setUser(null);
    setMnemonic(null);
    setPreferences(null);
  }, []);

  const updateProfile = useCallback(async (displayName: string) => {
    const u = await platform.updateProfile(displayName);
    setUser(u);
  }, []);

  const updatePreferences = useCallback(async (prefs: Partial<UserPreferences>) => {
    const p = await platform.updatePreferences(prefs);
    setPreferences(p);
  }, []);

  const clearError = useCallback(() => setError(null), []);

  const session = useMemo<Session | null>(() => {
    if (!mnemonic || !user) return null;
    return { mnemonic, publicKeyHex: user.public_key_hex, chainId };
  }, [mnemonic, user, chainId]);

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      preferences,
      mnemonic,
      chainId,
      loading,
      error,
      session,
      register,
      login,
      logout,
      updateProfile,
      updatePreferences,
      clearError,
    }),
    [user, preferences, mnemonic, chainId, loading, error, session, register, login, logout, updateProfile, updatePreferences, clearError],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within AuthProvider');
  return ctx;
}

export function useSession(): Session | null {
  return useAuth().session;
}
