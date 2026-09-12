-- migrations/20240101_001_initial_schema.sql

-- ─── Developers ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS developers (
    id            UUID PRIMARY KEY,
    name          TEXT NOT NULL,
    email         TEXT NOT NULL UNIQUE,
    company       TEXT,
    tier          TEXT NOT NULL DEFAULT 'free',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_developers_email ON developers(email);
CREATE INDEX IF NOT EXISTS idx_developers_tier   ON developers(tier);

-- ─── API Keys ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS api_keys (
    id            UUID PRIMARY KEY,
    developer_id  UUID NOT NULL REFERENCES developers(id) ON DELETE CASCADE,
    key_prefix    TEXT NOT NULL,          -- first 12 chars (for display)
    key_hash      TEXT NOT NULL,          -- sha256(full_key)
    label         TEXT NOT NULL DEFAULT 'default',
    tier          TEXT NOT NULL DEFAULT 'free',
    scopes        JSONB DEFAULT '[]'::jsonb,
    revoked       BOOLEAN NOT NULL DEFAULT false,
    last_used_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_api_keys_hash     ON api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_dev      ON api_keys(developer_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_revoked  ON api_keys(revoked) WHERE revoked = false;

-- ─── API Usage counters ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS api_usage (
    developer_id  UUID NOT NULL REFERENCES developers(id),
    api_key_id    BIGINT,
    period_start  DATE NOT NULL,          -- first day of billing period
    call_count    BIGINT NOT NULL DEFAULT 0,
    last_method   TEXT,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (developer_id, period_start)
);

-- ─── Billing events ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS billing_events (
    id            UUID PRIMARY KEY,
    developer_id  UUID NOT NULL REFERENCES developers(id),
    api_key_id    BIGINT,
    event_type    TEXT NOT NULL,          -- 'overage_charge' | 'subscription_charge' | 'payment_received'
    amount_augesat BIGINT NOT NULL DEFAULT 0,
    overage_calls BIGINT,
    method        TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_billing_events_dev ON billing_events(developer_id, created_at);

-- ─── Invoices ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS invoices (
    id            UUID PRIMARY KEY,
    developer_id  UUID NOT NULL REFERENCES developers(id),
    period_start  DATE NOT NULL,
    period_end    DATE NOT NULL,
    calls         BIGINT NOT NULL DEFAULT 0,
    total_augesat BIGINT NOT NULL DEFAULT 0,
    status        TEXT NOT NULL DEFAULT 'pending', -- pending | paid | overdue | cancelled
    paid_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_invoices_dev_period ON invoices(developer_id, period_start);

-- ─── Webhooks ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS webhooks (
    id            UUID PRIMARY KEY,
    developer_id  UUID NOT NULL REFERENCES developers(id),
    url           TEXT NOT NULL,
    secret        TEXT,                   -- HMAC-SHA256 signing secret
    events        JSONB NOT NULL DEFAULT '[]'::jsonb,
    active        BOOLEAN NOT NULL DEFAULT true,
    last_delivered_at TIMESTAMPTZ,
    last_status    INT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Payment sessions (idempotency / tracking) ───────────────────────────────
CREATE TABLE IF NOT EXISTS payment_sessions (
    id              UUID PRIMARY KEY,
    developer_id    UUID NOT NULL REFERENCES developers(id),
    merchant_account TEXT NOT NULL,
    amount_augesat  BIGINT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending', -- pending | confirming | confirmed | expired | failed
    payer_account   BIGINT,
    transaction_hash TEXT,
    block_number    BIGINT,
    metadata        JSONB DEFAULT '{}'::jsonb,
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    confirmed_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_payment_sessions_dev ON payment_sessions(developer_id, created_at);
