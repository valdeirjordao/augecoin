-- AUGECOIN ops — add a 'lifetime' license plan.
--
-- The `plan` column is guarded by a CHECK constraint. Lifetime licenses never
-- expire (far-future `expires_at`), used for the network's genesis validators
-- and other permanent operators. Extending the allowed values keeps the
-- DB-layer guarantee while allowing the new plan.

ALTER TABLE licenses DROP CONSTRAINT IF EXISTS licenses_plan_check;
ALTER TABLE licenses ADD CONSTRAINT licenses_plan_check
  CHECK (plan IN ('monthly', 'semiannual', 'annual', 'lifetime'));
