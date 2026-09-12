-- AUGECOIN ops — releases (OTA) and monitoring alerts.
--
-- Design notes:
--   * `releases.artifact_hash` is the BLAKE3-256 of the installer artifact;
--     `signature` is an Ed25519 signature over the raw 32-byte artifact hash,
--     produced by the release signing key (held by the release pipeline, never
--     by this service). Clients verify the signature with a baked-in public key
--     before installing anything.
--   * `alerts` is append-only; `resolved_at` marks closure. The monitor keeps at
--     most one *unresolved* alert per (validator, kind).

CREATE TABLE IF NOT EXISTS releases (
    id            UUID PRIMARY KEY,
    version       TEXT NOT NULL,
    platform      TEXT NOT NULL
                  CHECK (platform IN ('windows', 'linux')),
    artifact_url  TEXT,
    artifact_hash TEXT NOT NULL,
    signature     TEXT NOT NULL,
    notes         TEXT,
    published_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (version, platform)
);

CREATE INDEX IF NOT EXISTS idx_releases_platform ON releases (platform, published_at);

CREATE TABLE IF NOT EXISTS alerts (
    id           UUID PRIMARY KEY,
    validator_id UUID NOT NULL REFERENCES validators (id) ON DELETE CASCADE,
    kind         TEXT NOT NULL
                 CHECK (kind IN ('offline_warning', 'offline_critical', 'poa_removal')),
    message      TEXT,
    resolved_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_alerts_validator ON alerts (validator_id, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_alerts_unresolved ON alerts (validator_id, kind) WHERE resolved_at IS NULL;
