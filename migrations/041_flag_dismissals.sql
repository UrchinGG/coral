CREATE TABLE IF NOT EXISTS flag_dismissals (
  flag_key        text PRIMARY KEY,
  dismissed_until timestamptz NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_flag_dismissals_until ON flag_dismissals (dismissed_until);
