-- AUGECOIN ops — network-wide reward ledger.
--
-- Unlike `validator_rewards` (which attributes each block to a *SaaS-enrolled*
-- validator by `leader_id`), this ledger records EVERY block the node produces,
-- independent of whether the leader is enrolled in the SaaS. It is the source of
-- truth for network-wide emission KPIs, so the dashboard matches the chain even
-- when blocks are led by genesis validators or validators not managed by the SaaS.

CREATE TABLE IF NOT EXISTS network_rewards (
    block_number BIGINT PRIMARY KEY,
    auge         BIGINT NOT NULL DEFAULT 0,
    fees         BIGINT NOT NULL DEFAULT 0,
    augeids      BIGINT NOT NULL DEFAULT 0,
    block_ts     TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_network_rewards_ts ON network_rewards (block_ts);
