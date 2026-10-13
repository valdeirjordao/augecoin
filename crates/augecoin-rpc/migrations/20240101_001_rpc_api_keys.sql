-- Persistent API key store for the RPC layer.
-- Keys survive node restarts and are shared across nodes via PostgreSQL.
-- Key hashes are computed by the application (SHA-256 hex).
--
-- Apply with: sqlx migrate run (from augecoin-rpc or augecoin-ops)

CREATE TABLE IF NOT EXISTS rpc_api_keys (
    id              SERIAL PRIMARY KEY,
    key_hash        VARCHAR(64) NOT NULL UNIQUE,
    key_prefix      VARCHAR(32) NOT NULL,
    is_admin        BOOLEAN NOT NULL DEFAULT FALSE,
    label           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ,
    revoked         BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_rpc_api_keys_hash
    ON rpc_api_keys (key_hash);

CREATE INDEX IF NOT EXISTS idx_rpc_api_keys_prefix
    ON rpc_api_keys (key_prefix);

-- Bootstrap admin key (hash of "test-admin-key" = SHA-256)
INSERT INTO rpc_api_keys (key_hash, key_prefix, is_admin, label)
SELECT '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08',
       'test-admin-key', TRUE, 'bootstrap admin'
WHERE NOT EXISTS (
    SELECT 1 FROM rpc_api_keys
    WHERE key_hash = '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08'
);
