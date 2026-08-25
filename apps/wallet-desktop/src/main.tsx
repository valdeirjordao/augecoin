import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

const s = document.createElement('style');
s.textContent = `
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f0f1a; color: #e0e0e0; min-height: 100vh; }
  #root { max-width: 420px; margin: 0 auto; padding: 20px; }
  .screen { display: flex; flex-direction: column; gap: 20px; padding: 20px 0; }
  h1 { font-size: 28px; color: #7c5cfc; text-align: center; }
  h2 { font-size: 22px; color: #a78bfa; }
  .subtitle { text-align: center; color: #888; }
  .actions { display: flex; flex-direction: column; gap: 12px; margin-top: 20px; }
  button { padding: 14px 24px; border: none; border-radius: 12px; font-size: 16px; cursor: pointer; font-weight: 600; }
  button:disabled { opacity: 0.4; cursor: not-allowed; }
  button.primary { background: #7c5cfc; color: white; }
  button.active { background: #7c5cfc; color: white; }
  .warning { background: #332200; border: 1px solid #664400; color: #ffa500; padding: 12px; border-radius: 8px; font-size: 14px; }
  .mnemonic-box { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; background: #1a1a2e; padding: 16px; border-radius: 12px; border: 1px solid #2a2a3d; }
  .mnemonic-word { font-size: 14px; color: #a78bfa; font-family: monospace; }
  .checkbox-label { display: flex; align-items: center; gap: 8px; font-size: 14px; cursor: pointer; }
  .text-input { padding: 12px; border-radius: 8px; border: 1px solid #2a2a3d; background: #1a1a2e; color: #e0e0e0; font-size: 16px; width: 100%; }
  textarea.text-input { resize: vertical; font-family: monospace; }
  .error { color: #ff4444; font-size: 14px; }
  .balance-card { background: linear-gradient(135deg, #7c5cfc, #a78bfa); padding: 24px; border-radius: 16px; text-align: center; }
  .balance-label { font-size: 14px; opacity: 0.8; }
  .balance-value { font-size: 32px; font-weight: 700; display: block; margin-top: 8px; }
  .tabs { display: flex; gap: 4px; background: #1a1a2e; padding: 4px; border-radius: 12px; }
  .tabs button { flex: 1; padding: 10px; background: transparent; color: #888; font-size: 14px; border-radius: 8px; }
  .tab-content { padding: 10px 0; display: flex; flex-direction: column; gap: 12px; }
  .account-row { display: flex; justify-content: space-between; padding: 12px; background: #1a1a2e; border-radius: 8px; }
  code { background: #1a1a2e; padding: 4px 8px; border-radius: 4px; font-size: 14px; }
`;
document.head.appendChild(s);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode><App /></React.StrictMode>
);
