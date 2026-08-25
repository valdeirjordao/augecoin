// AUGECOIN Wallet Web — client for the operational backend (augecoin-ops).
//
// The wallet server is a trusted backend on the same infrastructure as the ops
// service. It holds the ops ADMIN key server-side (never in the browser) and
// uses it only to issue licenses and read the license/validator/stats owned by
// the authenticated user. The browser never sees this key.

const OPS_API_URL = process.env.AUGECOIN_OPS_API_URL || 'http://127.0.0.1:8790';
const OPS_ADMIN_KEY = process.env.AUGECOIN_OPS_ADMIN_KEY || '';

export class OpsError extends Error {
  constructor(status, message) {
    super(message || `ops HTTP ${status}`);
    this.name = 'OpsError';
    this.status = status;
  }
}

async function opsRequest(method, path, body) {
  const headers = { 'Content-Type': 'application/json' };
  if (OPS_ADMIN_KEY) headers['x-api-key'] = OPS_ADMIN_KEY;

  const res = await fetch(`${OPS_API_URL}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  let data = null;
  try { data = await res.json(); } catch { /* empty */ }

  if (!res.ok) {
    throw new OpsError(res.status, data && data.error ? data.error : `HTTP ${res.status}`);
  }
  return data;
}

export const ops = {
  get: (path) => opsRequest('GET', path),
  post: (path, body) => opsRequest('POST', path, body || {}),
};

/** Plans served to the purchase page (single source of truth). */
export const PLANS = {
  monthly: { id: 'monthly', label: 'Mensal', usd: 15, discount: null },
  semiannual: { id: 'semiannual', label: 'Semestral', usd: 75, discount: '2 meses grátis' },
  annual: { id: 'annual', label: 'Anual', usd: 120, discount: '4 meses grátis' },
};
