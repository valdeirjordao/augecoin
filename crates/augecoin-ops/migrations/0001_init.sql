-- AUGECOIN ops — initial schema for the validator licensing domain.
--
-- Design notes:
--   * `license_key` is NEVER stored in plaintext. We store its BLAKE3-256 hash
--     (license_key_hash) plus the first 4-char group (license_key_prefix) so the
--     admin UI can display a recognizable fragment without exposing the secret.
--   * `plan` / `status` are TEXT columns guarded by CHECK constraints rather than
--     native PG enums. This keeps the Rust mapping trivial and makes adding a new
--     plan/status a single ALTER (ADD VALUE on an enum cannot run inside a
--     transaction). The CHECK keeps integrity guarantees at the DB layer.
--   * `user_id` is a soft reference to the wallet platform user (lives in the
--     wallet-web SQLite), deliberately NOT a foreign key: the two services are
--     separate persistence boundaries.

CREATE TABLE IF NOT EXISTS licenses (
    id                 UUID PRIMARY KEY,
    license_key_hash   TEXT NOT NULL UNIQUE,
    license_key_prefix CHAR(4) NOT NULL,
    user_id            UUID NOT NULL,
    plan               TEXT NOT NULL CHECK (plan IN ('monthly', 'semiannual', 'annual')),
    augeid             TEXT,
    machine_hash       TEXT,
    public_key         TEXT,
    ip                 TEXT,
    expires_at         TIMESTAMPTZ NOT NULL,
    status             TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'expired', 'suspended', 'revoked')),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_licenses_user ON licenses (user_id);
CREATE INDEX IF NOT EXISTS idx_licenses_status ON licenses (status);
CREATE INDEX IF NOT EXISTS idx_licenses_expires_at ON licenses (expires_at);

-- Append-only, immutable audit trail. Every mutation of a license inserts a row
-- here; rows are never updated or deleted by application code.
CREATE TABLE IF NOT EXISTS license_events (
    id         BIGSERIAL PRIMARY KEY,
    license_id UUID NOT NULL REFERENCES licenses (id) ON DELETE CASCADE,
    event      TEXT NOT NULL,
    actor      TEXT NOT NULL,
    data       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_license_events_license ON license_events (license_id, created_at);
