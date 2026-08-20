import React from 'react';
import { BrowserRouter, MemoryRouter, Navigate, Route, Routes } from 'react-router-dom';
import { AuthProvider, useAuth } from './app/AuthContext';
import { ToastProvider } from './app/ToastContext';
import { Layout } from './components/Layout';
import { Login } from './app/Login';
import { Dashboard } from './app/Dashboard';
import { Marketplace } from './app/Marketplace';
import { MyWallets } from './app/MyWallets';
import { Profile } from './app/Profile';
import { History } from './app/History';
import { Gifts } from './app/Gifts';
import { Send } from './app/Send';
import { Receive } from './app/Receive';
import { SettingsName } from './app/SettingsName';
import { Validator } from './app/Validator';
import { ValidatorPlans } from './app/ValidatorPlans';
import { ValidatorLicense } from './app/ValidatorLicense';
import { ValidatorDashboard } from './app/ValidatorDashboard';

function RequireAuth({ children }: { children: React.ReactNode }) {
  const { session } = useAuth();
  if (!session) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export function AppProviders({ children }: { children: React.ReactNode }) {
  return (
    <AuthProvider>
      <ToastProvider>{children}</ToastProvider>
    </AuthProvider>
  );
}

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route
        element={
          <RequireAuth>
            <Layout />
          </RequireAuth>
        }
      >
        <Route path="/" element={<Dashboard />} />
        <Route path="/marketplace" element={<Marketplace />} />
        <Route path="/gifts" element={<Gifts />} />
        <Route path="/profile" element={<Profile />} />
        <Route path="/my-wallets" element={<MyWallets />} />
        <Route path="/send" element={<Send />} />
        <Route path="/receive" element={<Receive />} />
        <Route path="/history" element={<History />} />
        <Route path="/settings/name" element={<SettingsName />} />
        <Route path="/validator" element={<Validator />} />
        <Route path="/validator/plans" element={<ValidatorPlans />} />
        <Route path="/validator/license" element={<ValidatorLicense />} />
        <Route path="/validator/dashboard" element={<ValidatorDashboard />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

export function App({ router = 'browser' }: { router?: 'browser' | 'memory' }) {
  const content = (
    <AppProviders>
      <AppRoutes />
    </AppProviders>
  );
  if (router === 'memory') {
    return <MemoryRouter initialEntries={['/']}>{content}</MemoryRouter>;
  }
  return <BrowserRouter>{content}</BrowserRouter>;
}

export default App;
