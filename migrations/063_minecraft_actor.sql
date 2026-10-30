SET LOCAL lock_timeout = '3s';

ALTER TABLE api_request_log ADD COLUMN IF NOT EXISTS actor_uuid uuid;

CREATE INDEX IF NOT EXISTS idx_arl_actor_uuid ON ONLY api_request_log (actor_uuid, ts DESC)
  WHERE actor_uuid IS NOT NULL;
