import { useAuth } from '../app/AuthContext';

/**
 * Camada 1 — Conta da Plataforma.
 *
 * Exposes the platform identity (login/cadastro/perfil/preferências/sessão).
 * This layer has no balance and never creates an AUGEID.
 */
export function usePlatformUser() {
  const {
    user,
    preferences,
    loading,
    error,
    register,
    login,
    logout,
    updateProfile,
    updatePreferences,
    clearError,
  } = useAuth();

  return {
    user,
    preferences,
    loading,
    error,
    register,
    login,
    logout,
    updateProfile,
    updatePreferences,
    clearError,
  };
}
