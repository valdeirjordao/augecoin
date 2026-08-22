// AUGECOIN Wallet Web — SQLite schema (node:sqlite, zero native deps).
//
// Three layers, strictly separated:
//   platform_users    -> Camada 1: Conta da Plataforma (login/cadastro/perfil).
//   user_preferences  -> Camada 1: preferências (tema/idioma/notificações).
//   linked_wallets    -> Wallet Registry: ponte usuário -> AUGEID (account_number).
//
// The registry never stores balance, state or private keys. On-chain truth
// (balance, name, ownership, transfers) lives exclusively in the SafeBox; the
// wallet-web frontend reads it via the official node RPC (`getaccount`).

import { DatabaseSync } from 'node:sqlite';
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const DB_PATH = process.env.AUGECOIN_WALLET_DB || resolve(__dirname, '../data/wallet-web.sqlite');

let db = null;

const SCHEMA = `
CREATE TABLE IF NOT EXISTS platform_users (
  id             TEXT PRIMARY KEY,
  email          TEXT NOT NULL UNIQUE,
  password_hash  TEXT NOT NULL,
  display_name   TEXT NOT NULL,
  public_key_hex TEXT NOT NULL DEFAULT '',
  address        TEXT NOT NULL DEFAULT '',
  augeid         TEXT,
  activation_tx  TEXT,
  first_receive  TEXT,
  created_at     TEXT NOT NULL,
  last_login     TEXT
);

CREATE TABLE IF NOT EXISTS user_preferences (
  user_id       TEXT PRIMARY KEY REFERENCES platform_users(id) ON DELETE CASCADE,
  theme         TEXT NOT NULL DEFAULT 'dark',
  language      TEXT NOT NULL DEFAULT 'pt-BR',
  notifications INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS linked_wallets (
  user_id        TEXT NOT NULL REFERENCES platform_users(id) ON DELETE CASCADE,
  account_number INTEGER NOT NULL,
  linked_at      TEXT NOT NULL,
  PRIMARY KEY (user_id, account_number)
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES platform_users(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL
);

-- Validator license purchase orders (Camada 2: SaaS de validação).
-- status: pending -> paid (operador confirma pagamento) -> issued (licença emitida).
CREATE TABLE IF NOT EXISTS validator_orders (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES platform_users(id) ON DELETE CASCADE,
  plan       TEXT NOT NULL CHECK (plan IN ('monthly', 'semiannual', 'annual')),
  method     TEXT NOT NULL CHECK (method IN ('pix', 'usdt', 'auge')),
  amount_usd INTEGER NOT NULL,
  status     TEXT NOT NULL DEFAULT 'pending'
             CHECK (status IN ('pending', 'paid', 'issued', 'cancelled')),
  license_id TEXT,
  license_key TEXT,
  created_at TEXT NOT NULL,
  paid_at    TEXT,
  issued_at  TEXT
);

-- Financial configuration (single row) for manual payment instructions
-- shown to the member on purchase and edited by the operator in the panel.
CREATE TABLE IF NOT EXISTS payment_config (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  pix_key       TEXT NOT NULL DEFAULT '',
  usdt_address  TEXT NOT NULL DEFAULT '',
  usdt_network  TEXT NOT NULL DEFAULT '',
  auge_address  TEXT NOT NULL DEFAULT '',
  auge_network  TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_refresh_user ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_linked_user ON linked_wallets(user_id);
CREATE INDEX IF NOT EXISTS idx_orders_user ON validator_orders(user_id);
`;

export function getDb() {
  if (db) return db;
  mkdirSync(dirname(DB_PATH), { recursive: true });
  db = new DatabaseSync(DB_PATH);
  db.exec('PRAGMA journal_mode = WAL;');
  db.exec('PRAGMA foreign_keys = ON;');
  db.exec(SCHEMA);
  migrate(db);
  return db;
}

/** Additive migrations for databases created before a column was introduced. */
function migrate(db) {
  const cols = db.prepare('PRAGMA table_info(validator_orders)').all();
  if (!cols.some((c) => c.name === 'license_key')) {
    db.exec('ALTER TABLE validator_orders ADD COLUMN license_key TEXT;');
  }
  const userCols = db.prepare('PRAGMA table_info(platform_users)').all();
  for (const [name, definition] of [['address', "TEXT NOT NULL DEFAULT ''"], ['augeid', 'TEXT'], ['activation_tx', 'TEXT'], ['first_receive', 'TEXT']]) {
    if (!userCols.some((c) => c.name === name)) db.exec(`ALTER TABLE platform_users ADD COLUMN ${name} ${definition};`);
  }
}

export function closeDb() {
  if (db) {
    db.close();
    db = null;
  }
}
