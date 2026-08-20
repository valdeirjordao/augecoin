-- AUGECOIN ops — validator domain: validators, heartbeats and audit events.
--
-- Design notes:
--   * Binding data (machine_hash, augeid, ip) lives on `licenses` (single source
--     of truth); `validators` holds only operational state keyed by public_key.
--   * `status` is the *lifecycle* state (pending/active/suspended/revoked).
--     Presence (online/offline) is NEVER stored — it is derived at read time
--     from `last_seen` against the heartbeat window, mirroring how the license
--     domain derives `expired` from `expires_at` instead of storing it.
--   * `public_key` holds the Ed25519 public key (64 hex chars). The matching
--     private key never leaves the operator machine and is never stored here.
--   * `uptime`, `blocks`, `blocks_lost`, `leadership`, `total_rewards` are
--     *mirrors* of on-chain metrics. The blockchain remains the authority; these
--     columns let the SaaS serve dashboards without holding balances or keys.

CREATE TABLE IF NOT EXISTS validators (
    id                UUID PRIMARY KEY,
    license_id        UUID NOT NULL UNIQUE REFERENCES licenses (id) ON DELETE RESTRICT,
    public_key        TEXT NOT NULL UNIQUE,
    node_validator_id BIGINT,
    os                TEXT,
    cpu               INT,
    ram               INT,
    version           TEXT,
    uptime            BIGINT NOT NULL DEFAULT 0,
    blocks            BIGINT NOT NULL DEFAULT 0,
    blocks_lost       BIGINT NOT NULL DEFAULT 0,
    leadership        BIGINT NOT NULL DEFAULT 0,
    total_rewards     BIGINT NOT NULL DEFAULT 0,
    last_seen         TIMESTAMPTZ,
    status            TEXT NOT NULL DEFAULT 'pending'
                      CHECK (status IN ('pending', 'active', 'suspended', 'revoked')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_validators_license ON validators (license_id);
CREATE INDEX IF NOT EXISTS idx_validators_status ON validators (status);
CREATE INDEX IF NOT EXISTS idx_validators_last_seen ON validators (last_seen);
CREATE INDEX IF NOT EXISTS idx_validators_public_key ON validators (public_key);

-- Time-series heartbeat samples (cpu/ram/block/uptime at each 30s tick).
-- Append-only; old rows are pruned by the retention job, never by app code.
CREATE TABLE IF NOT EXISTS heartbeats (
    id           UUID PRIMARY KEY,
    validator_id UUID NOT NULL REFERENCES validators (id) ON DELETE CASCADE,
    cpu          INT,
    ram          INT,
    block        BIGINT,
    uptime       BIGINT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_heartbeats_validator ON heartbeats (validator_id, created_at);

-- Append-only, immutable audit trail for validator lifecycle mutations.
-- Rows are never updated or deleted by application code.
CREATE TABLE IF NOT EXISTS validator_events (
    id           BIGSERIAL PRIMARY KEY,
    validator_id UUID NOT NULL REFERENCES validators (id) ON DELETE CASCADE,
    event        TEXT NOT NULL,
    actor        TEXT NOT NULL,
    data         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_validator_events_validator ON validator_events (validator_id, created_at);
