# AUGECOIN Explorer

Official block explorer for the AUGECOIN blockchain. Pure **HTML5 + CSS3 +
vanilla JavaScript (ES2023)** — no React/Vue/Angular, no build step, no runtime
dependencies.

Served at `https://explorer.augeco.in` and consuming the JSON-RPC API at
`https://www.augeco.in/rpc`.

## Features

- Dashboard with realtime height, TPS, supply and latest blocks/operations.
- Intelligent search (block height, AUGEID, account name, bech32 address, hash).
- Block, operation, account (AUGEID), validators, supply and rich-list pages.
- Dark/light theme, responsive (desktop/tablet/mobile).
- Dependency-free canvas charts (line, bar, donut) — no Chart.js.
- Copy/QR/CSV/share helpers, skeleton loading, toasts, pagination.
- CSP, HTML escaping, exponential-backoff RPC retry.

## Structure

```
explorer/
├── index.html           dashboard
├── blocks.html          block list
├── block.html           single block (?block=N)
├── transaction.html     single operation (?block=N&op=I)
├── account.html         account (?id=N | ?address=auge1… | ?name=…)
├── validators.html      validator set + production chart
├── supply.html          emission schedule + charts
├── richlist.html        top accounts by balance
├── search.html          fallback search results
├── 404.html
├── assets/
│   ├── css/             themes, main, dashboard, tables, responsive
│   ├── js/
│   │   ├── config.js     RPC URL + chain constants
│   │   ├── api/client.js JSON-RPC client (single source of truth)
│   │   ├── utils.js      formatting, escaping, bech32m + blake3
│   │   ├── blake3.js     self-contained blake3-512 (address derivation)
│   │   ├── theme.js      dark/light
│   │   ├── components.js header/footer/cards/badges/toast/modal/pagination
│   │   ├── charts.js     canvas charts
│   │   ├── search.js     intelligent search routing
│   │   ├── realtime.js   pollers + TPS tracker
│   │   ├── app.js        bootstrap + dispatcher
│   │   └── pages/*.js    one module per page
│   ├── icons/ logo.svg
│   └── images/ favicon.svg
├── manifest.webmanifest
├── robots.txt
└── sitemap.xml
```

## Configuration

Edit `assets/js/config.js` to change the RPC endpoint and chain constants. The
RPC URL can also be overridden at runtime with `?rpc=<url>` or
`localStorage.setItem('auge_rpc', '<url>')`.

Chain constants are derived from `crates/augecoin-core/src/emission.rs`:
total supply `762,120,000 AUGE`, block reward `29 AUGE`, block time `60s`,
linear emission (no halving), `26,280,000` emission blocks.

## Deployment

It is a fully static site — serve the directory with any static server.

```bash
# nginx (recommended)
server {
  listen 80;
  listen [::]:80;
  server_name explorer.augeco.in;
  root /opt/augecoin/explorer;
  index index.html;
  location / { try_files $uri $uri/ /index.html; }
}
```

### RPC + CORS

The explorer runs in the browser and calls `https://www.augeco.in/rpc` via
`fetch`. Two requirements:

1. **CORS** — the reverse proxy in front of the node must return
   `Access-Control-Allow-Origin: https://explorer.augeco.in` (or `*`) and
   `Access-Control-Allow-Headers: Content-Type`, `Access-Control-Allow-Methods: POST`.
   Example nginx snippet:

   ```nginx
   location /rpc {
     proxy_pass https://127.0.0.1:9005/;
     add_header Access-Control-Allow-Origin "https://explorer.augeco.in" always;
     add_header Access-Control-Allow-Methods "POST, GET, OPTIONS" always;
     add_header Access-Control-Allow-Headers "Content-Type" always;
     if ($request_method = OPTIONS) { return 204; }
   }
   ```

2. **TLS** — the node RPC speaks TLS. Terminate TLS at the proxy with a trusted
   certificate (or proxy with `proxy_ssl_verify off` for the node's self-signed
   cert).

If the RPC endpoint changes, update the `connect-src` directive in the CSP
`<meta>` tags as well.

## Development

Serve locally and point at a running node:

```bash
cd explorer
python3 -m http.server 8080
# open http://localhost:8080/?rpc=http://127.0.0.1:9005
```

## Notes & API limitations

The JSON-RPC API does not expose a standalone transaction hash or a reverse
index. The explorer therefore:

- identifies operations as `block·index` (block height + position);
- resolves block "hash" via the on-chain `operations_hash` (merkle root);
- performs bounded reverse scans for hash lookups (see `findBlockByHash`).

The bech32 address (`auge1…`) is derived client-side from the account's Ed25519
public key using a self-contained blake3-512 + bech32m implementation, verified
byte-for-byte against the Rust core cross-language vectors.
