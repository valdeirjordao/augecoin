import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

const s = document.createElement('style');
s.textContent = `
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0f172a; color: #e2e8f0; min-height: 100vh;
  }
  #root { max-width: 760px; margin: 0 auto; padding: 24px; }
  h1 { font-size: 24px; color: #f97316; }
  h2 { font-size: 18px; color: #f8fafc; }
  .muted { color: #94a3b8; }
  .mono { font-family: 'SFMono-Regular', Consolas, monospace; }
  .screen { display: flex; flex-direction: column; gap: 20px; padding: 20px 0; }
  .card { background: #1e293b; border: 1px solid #2b3a55; border-radius: 12px; padding: 20px; }
  .label { display: block; font-size: 12px; text-transform: uppercase; letter-spacing: 1px; color: #94a3b8; margin-bottom: 6px; font-weight: 600; }
  .input { width: 100%; padding: 12px; border-radius: 8px; border: 1px solid #2b3a55; background: #0f172a; color: #e2e8f0; font-size: 15px; font-family: monospace; }
  .input:focus { outline: none; border-color: #f97316; }
  .actions { display: flex; gap: 10px; flex-wrap: wrap; }
  button { padding: 12px 22px; border: none; border-radius: 10px; font-size: 15px; cursor: pointer; font-weight: 600; background: #1e293b; color: #e2e8f0; border: 1px solid #2b3a55; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  button.primary { background: #f97316; border-color: #f97316; color: #fff; }
  button.primary:hover { background: #ea580c; }
  .error { color: #ef4444; font-size: 14px; padding: 10px 12px; border: 1px solid #ef4444; border-radius: 8px; background: rgba(239,68,68,0.08); }
  .success { color: #22c55e; }
  .stat-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; }
  .stat { background: #1e293b; border: 1px solid #2b3a55; border-radius: 10px; padding: 14px; }
  .stat .v { font-size: 22px; font-weight: 700; color: #f97316; margin-top: 4px; }
  .badge { display: inline-block; padding: 3px 10px; border-radius: 999px; font-size: 12px; font-weight: 600; }
  .badge-green { background: rgba(34,197,94,0.15); color: #22c55e; }
  .badge-amber { background: rgba(245,158,11,0.15); color: #f59e0b; }
  .badge-red { background: rgba(239,68,68,0.15); color: #ef4444; }
  .badge-gray { background: #243149; color: #94a3b8; }
  .row { display: flex; justify-content: space-between; gap: 12px; padding: 8px 0; border-bottom: 1px solid #2b3a55; }
  .row:last-child { border-bottom: none; }
  .kv-k { color: #94a3b8; font-size: 13px; }
  .kv-v { font-weight: 600; text-align: right; word-break: break-all; }
  .leader { text-align: center; padding: 20px; border-radius: 12px; background: linear-gradient(135deg, rgba(249,115,22,0.25), rgba(249,115,22,0.05)); border: 1px solid #f97316; animation: pulse 1.6s infinite; }
  @keyframes pulse { 0%, 100% { box-shadow: 0 0 0 0 rgba(249,115,22,0.4); } 50% { box-shadow: 0 0 24px 4px rgba(249,115,22,0.25); } }
  .progress { display: flex; flex-direction: column; gap: 8px; margin-top: 16px; }
  .bar { height: 8px; background: #243149; border-radius: 999px; overflow: hidden; }
  .bar > div { height: 100%; background: #f97316; transition: width 0.4s ease; }
`;
document.head.appendChild(s);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode><App /></React.StrictMode>
);
