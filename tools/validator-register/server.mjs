// AUGECOIN Validator Registration service.
// Two concerns:
//   1. POST /api/validator/register  - submit a validatoradd consensus op for a
//      public key (idempotent: if already present, returns ok).
//   2. GET  /api/validator/lookup?key=<hex> - resolve the ValidatorSet id
//      assigned to a public key (after the add op commits to a block).
// Served by nginx at https://www.augeco.in/api/validator/...

import express from 'express';
import https from 'node:https';

const PORT = Number(process.env.AUGECOIN_VALIDATOR_REG_PORT || 8788);
const REG_TOKEN = process.env.AUGECOIN_VALIDATOR_REG_TOKEN || 'change-me-validator-reg-token';
const NODE_RPC = process.env.AUGECOIN_NODE_RPC || 'https://127.0.0.1:9005/';
const ADMIN_API_KEY = process.env.AUGECOIN_ADMIN_API_KEYS || '';

const app = express();
app.use(express.json({ limit: '16kb' }));

function rpcCall(method, params) {
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({ jsonrpc: '2.0', method, params, id: 1 });
    const url = new URL(NODE_RPC);
    const req = https.request(
      {
        hostname: url.hostname,
        port: url.port || 443,
        path: url.pathname || '/',
        method: 'POST',
        rejectUnauthorized: false,
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(body),
          ...(ADMIN_API_KEY ? { 'x-api-key': ADMIN_API_KEY } : {}),
        },
      },
      (res) => {
        let data = '';
        res.on('data', (c) => (data += c));
        res.on('end', () => {
          try {
            resolve(JSON.parse(data));
          } catch (e) {
            reject(new Error('invalid rpc response'));
          }
        });
      },
    );
    req.on('error', reject);
    req.write(body);
    req.end();
  });
}

async function currentHeight() {
  const res = await rpcCall('nodestatus', {});
  return res?.result?.current_height ?? 0;
}

async function findValidatorByKey(publicKeyHex) {
  const key = String(publicKeyHex || '').toLowerCase();
  const res = await rpcCall('getvalidatorset', {});
  const active = res?.result?.active || [];
  for (const v of active) {
    if (String(v.ed25519_public_key_hex).toLowerCase() === key) {
      return { id: v.id, status: v.status, key: v.ed25519_public_key_hex };
    }
  }
  return null;
}

app.post('/api/validator/register', async (req, res) => {
  try {
    const { ed25519_public_key_hex, token, name } = req.body || {};
    if (token !== REG_TOKEN) {
      return res.status(401).json({ ok: false, error: 'token de registro invalido' });
    }
    if (!/^[0-9a-f]{64}$/i.test(String(ed25519_public_key_hex || ''))) {
      return res.status(400).json({ ok: false, error: 'chave publica invalida (64 hex)' });
    }
    const key = String(ed25519_public_key_hex).toLowerCase();

    // Already registered? return the assigned id.
    const existing = await findValidatorByKey(key);
    if (existing) {
      return res.json({ ok: true, already: true, ...existing });
    }

    const height = await currentHeight();
    const result = await rpcCall('validatoradd', {
      ed25519_public_key_hex: key,
      activation_height: height,
    });

    if (result?.result?.success) {
      return res.json({
        ok: true,
        pending: true,
        message: 'validador enviado para registro',
        height,
        name: String(name || ''),
      });
    }
    return res.status(422).json({ ok: false, error: result?.result?.error || 'falha ao registrar', detail: result });
  } catch (e) {
    return res.status(500).json({ ok: false, error: e.message });
  }
});

// Look up the assigned ValidatorSet id for a public key.
app.get('/api/validator/lookup', async (req, res) => {
  try {
    const key = String(req.query.key || '').toLowerCase();
    if (!/^[0-9a-f]{64}$/.test(key)) {
      return res.status(400).json({ ok: false, error: 'chave publica invalida' });
    }
    const found = await findValidatorByKey(key);
    if (found) {
      return res.json({ ok: true, registered: true, ...found });
    }
    return res.json({ ok: true, registered: false, key });
  } catch (e) {
    return res.status(500).json({ ok: false, error: e.message });
  }
});

app.get('/health', (_req, res) => res.json({ ok: true }));

app.listen(PORT, () => {
  console.log(`[validator-register] listening on :${PORT}`);
});
