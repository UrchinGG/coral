ALTER TABLE api_request_log ADD COLUMN IF NOT EXISTS request_id uuid;

CREATE TABLE IF NOT EXISTS api_request_detail (
  request_id  uuid NOT NULL,
  ts          timestamptz NOT NULL,
  member_id   bigint,
  headers     jsonb NOT NULL,
  body        text,
  body_bytes  integer NOT NULL
) PARTITION BY RANGE (ts);

CREATE INDEX IF NOT EXISTS idx_ard_request ON api_request_detail (request_id);
CREATE INDEX IF NOT EXISTS idx_ard_member ON api_request_detail (member_id, ts DESC);

DO $$
DECLARE d date := current_date;
BEGIN
  FOR i IN 0..2 LOOP
    EXECUTE format(
      'CREATE TABLE IF NOT EXISTS api_request_detail_%s PARTITION OF api_request_detail FOR VALUES FROM (%L) TO (%L)',
      to_char(d + i, 'YYYYMMDD'), (d + i)::text, (d + i + 1)::text);
  END LOOP;
END $$;
