// AUGECOIN Wallet Web — HTTP client for the platform backend.
//
// Talks to the wallet-web server (platform identity + wallet registry) using
// HttpOnly cookies for the access/refresh tokens and a double-submit CSRF token
// echoed in the `X-CSRF-Token` header. On a 401 the client transparently
// attempts a single refresh before surfacing the error.

import { CONFIG } from './config';

export class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}

function readCsrf(): string {
  try {
    const match = document.cookie.match(/(?:^|;\s*)auge_csrf=([^;]+)/);
    return match ? decodeURIComponent(match[1]) : '';
  } catch {
    return '';
  }
}

type HttpMethod = 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';

export async function apiFetch<T>(
  path: string,
  opts: { method?: HttpMethod; body?: unknown; csrf?: boolean } = {},
): Promise<T> {
  const { method = 'GET', body, csrf = false } = opts;
  const headers: Record<string, string> = {};
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (csrf) {
    const token = readCsrf();
    if (token) headers['X-CSRF-Token'] = token;
  }

  const res = await fetch(`${CONFIG.API_BASE_URL}${path}`, {
    method,
    headers,
    credentials: 'include',
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  if (res.status === 401 && path !== '/auth/refresh') {
    const refreshed = await fetch(`${CONFIG.API_BASE_URL}/auth/refresh`, {
      method: 'POST',
      credentials: 'include',
    });
    if (refreshed.ok) {
      return apiFetch<T>(path, opts);
    }
  }

  if (res.status === 204) return undefined as T;

  const data = await res.json().catch(() => null);
  if (!res.ok) {
    throw new ApiError((data && data.error) || `HTTP ${res.status}`, res.status);
  }
  return data as T;
}
