-- AUGECOIN ops — reward ledger (financial dashboard).
--
-- Design notes:
--   * This is an append-only *mirror* of on-chain rewards. The blockchain stays
--     the authority; rows are inserted idempotently by the sync worker.
--   * `auge` = block reward + fees credited to the leader, in augesat
--     (1 AUGE = 100 000 000 augesat). `fees` kept separately for transparency.
--   * `augeids` = number of new AUGEIDs (accounts) emitted in the block and
--     owned by the leader (10 per block by consensus rule).
--   * `leader_id` is the on-chain validator id; `validator_id` is the ops-side
--     FK resolved via `validators.node_validator_id`.
--   * `block_ts` is the consensus block timestamp (drives period bucketing);
--     `created_at` is ingestion time (audit only).
--   * UNIQUE(validator_id, block_number) makes re-syncing idempotent.

CREATE TABLE IF NOT EXISTS validator_rewards (
    id           UUID PRIMARY KEY,
    validator_id UUID NOT NULL REFERENCES validators (id) ON DELETE CASCADE,
    block_number BIGINT NOT NULL,
    leader_id    BIGINT NOT NULL,
    auge         BIGINT NOT NULL DEFAULT 0,
    fees         BIGINT NOT NULL DEFAULT 0,
    augeids      INT NOT NULL DEFAULT 0,
    block_ts     TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (validator_id, block_number)
);

CREATE INDEX IF NOT EXISTS idx_validator_rewards_validator_ts
    ON validator_rewards (validator_id, block_ts);
CREATE INDEX IF NOT EXISTS idx_validator_rewards_block
    ON validator_rewards (block_number);

-- Resumable cursor for the rewards sync worker. Single-row key/value table so
-- the worker can be restarted without re-walking the whole chain.
CREATE TABLE IF NOT EXISTS sync_state (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
