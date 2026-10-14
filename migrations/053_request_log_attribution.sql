SET LOCAL lock_timeout = '3s';

ALTER TABLE api_request_log
  ADD COLUMN IF NOT EXISTS route text,
  ADD COLUMN IF NOT EXISTS actor_id bigint,
  ADD COLUMN IF NOT EXISTS outcome_status smallint,
  ADD COLUMN IF NOT EXISTS cause smallint,
  ADD COLUMN IF NOT EXISTS subjects smallint,
  ADD COLUMN IF NOT EXISTS country text;

ALTER TABLE api_request_detail
  ADD COLUMN IF NOT EXISTS server_error text,
  ADD COLUMN IF NOT EXISTS body_complete boolean NOT NULL DEFAULT true;

CREATE INDEX IF NOT EXISTS idx_arl_route ON ONLY api_request_log (route, ts DESC);
CREATE INDEX IF NOT EXISTS idx_arl_actor ON ONLY api_request_log (actor_id, ts DESC) WHERE actor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_arl_outcome ON ONLY api_request_log (outcome_status, ts DESC) WHERE outcome_status IS NOT NULL;

DO $$
DECLARE p record;
BEGIN
  FOR p IN
    SELECT c.relname AS name FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
    WHERE i.inhparent = 'api_request_log'::regclass
      AND c.relname ~ '^api_request_log_[0-9]{8}$'
      AND to_date(right(c.relname, 8), 'YYYYMMDD') > (now() AT TIME ZONE 'UTC')::date
  LOOP
    EXECUTE format('CREATE INDEX IF NOT EXISTS %I ON %I (route, ts DESC)', p.name || '_route_ts_idx', p.name);
    EXECUTE format('ALTER INDEX idx_arl_route ATTACH PARTITION %I', p.name || '_route_ts_idx');
    EXECUTE format('CREATE INDEX IF NOT EXISTS %I ON %I (actor_id, ts DESC) WHERE actor_id IS NOT NULL', p.name || '_actor_id_ts_idx', p.name);
    EXECUTE format('ALTER INDEX idx_arl_actor ATTACH PARTITION %I', p.name || '_actor_id_ts_idx');
    EXECUTE format('CREATE INDEX IF NOT EXISTS %I ON %I (outcome_status, ts DESC) WHERE outcome_status IS NOT NULL', p.name || '_outcome_status_ts_idx', p.name);
    EXECUTE format('ALTER INDEX idx_arl_outcome ATTACH PARTITION %I', p.name || '_outcome_status_ts_idx');
  END LOOP;
END $$;
