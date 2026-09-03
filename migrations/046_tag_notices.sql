CREATE TABLE IF NOT EXISTS pending_tag_notices (
  id           bigserial PRIMARY KEY,
  discord_id   bigint NOT NULL,
  uuid         text NOT NULL,
  username     text NOT NULL,
  tag_type     text NOT NULL,
  reason       text NOT NULL,
  created_at   timestamptz NOT NULL DEFAULT now(),
  delivered_at timestamptz
);

CREATE INDEX IF NOT EXISTS idx_pending_tag_notices_undelivered
  ON pending_tag_notices (discord_id)
  WHERE delivered_at IS NULL;
