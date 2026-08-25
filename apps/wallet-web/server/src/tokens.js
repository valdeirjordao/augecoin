// Token layer: JWT access token + rotating refresh token + CSRF double-submit.
//
// - Access token:  short-lived JWT (HMAC-SHA256), delivered as an HttpOnly,
//   SameSite=Lax cookie. Stateless.
// - Refresh token: opaque random value, stored *hashed* (sha256) in the DB,
//   rotated on every refresh. Revoking all of a user's rows == global logout.
// - CSRF:          double-submit token (non-HttpOnly cookie) that the SPA echoes
//                  in the `X-CSRF-Token` header on mutating requests; verified
//                  against the cookie + an Origin/Referer allowlist.

import { randomBytes, createHash } from 'node:crypto';
import jwt from 'jsonwebtoken';
import { getDb } from './db.js';

const JWT_SECRET = process.env.AUGECOIN_WALLET_JWT_SECRET || 'dev-insecure-secret-change-me';
const ACCESS_TTL = '15m';
const REFRESH_TTL_MS = 7 * 24 * 60 * 60 * 1000; // 7 days

export const COOKIE_NAMES = {
  access: 'auge_access',
  refresh: 'auge_refresh',
  csrf: 'auge_csrf',
};

const cookieBase = { httpOnly: true, sameSite: 'lax', secure: false, path: '/' };

export function cookieOpts() {
  return { ...cookieBase, secure: process.env.AUGECOIN_WALLET_SECURE_COOKIES === '1' };
}

export function signAccessToken(user) {
  return jwt.sign({ sub: user.id, email: user.email }, JWT_SECRET, { expiresIn: ACCESS_TTL });
}

export function verifyAccessToken(token) {
  try {
    return jwt.verify(token, JWT_SECRET);
  } catch {
    return null;
  }
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

export function newCsrfToken() {
  return randomBytes(32).toString('hex');
}

export function newRefreshToken(userId) {
  const token = randomBytes(48).toString('hex');
  const db = getDb();
  db.prepare(
    'INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)',
  ).run(
    randomBytes(16).toString('hex'),
    userId,
    sha256(token),
    new Date(Date.now() + REFRESH_TTL_MS).toISOString(),
    new Date().toISOString(),
  );
  return token;
}

/** Rotate a refresh token; returns the user id or null when invalid/expired. */
export function consumeRefreshToken(token) {
  if (!token) return null;
  const db = getDb();
  const row = db
    .prepare('SELECT id, user_id, expires_at FROM refresh_tokens WHERE token_hash = ?')
    .get(sha256(token));
  if (!row) return null;
  db.prepare('DELETE FROM refresh_tokens WHERE id = ?').run(row.id);
  if (new Date(row.expires_at).getTime() < Date.now()) return null;
  return row.user_id;
}

/** Global logout: revoke every refresh token for the user. */
export function revokeAllRefreshTokens(userId) {
  const db = getDb();
  db.prepare('DELETE FROM refresh_tokens WHERE user_id = ?').run(userId);
}
