SET LOCAL lock_timeout = '3s';

ALTER TABLE review
  ADD COLUMN IF NOT EXISTS player_star   integer,
  ADD COLUMN IF NOT EXISTS player_fkdr   double precision,
  ADD COLUMN IF NOT EXISTS confirm_state text,
  ADD COLUMN IF NOT EXISTS confirm_by    bigint;
