// AUGECOIN Operacional — REST client for the augecoin-ops API.
//
// Every admin request carries the API key as `x-api-key`. Errors are normalized
// to `ApiError` with the server-provided message (never leaking internals).

import { CONFIG, getApiKey } from './config.js';

export class ApiError extends Error {
  constructor(status, message) {
    super(message || `HTTP ${status}`);
    this.name = 'ApiError';
    this.status = status;
  }
}

async function request(method, path, body) {
  const headers = { 'Content-Type': 'application/json' };
  const key = getApiKey();
  if (key) headers['x-api-key'] = key;

  const res = await fetch(CONFIG.OPS_API_URL + path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  let data = null;
  try { data = await res.json(); } catch { /* empty body */ }

  if (!res.ok) {
    const msg = data && data.error ? data.error : `HTTP ${res.status}`;
    throw new ApiError(res.status, msg);
  }
  return data;
}

export const opsApi = {
  get: (path) => request('GET', path),
  post: (path, body) => request('POST', path, body || {}),
};
