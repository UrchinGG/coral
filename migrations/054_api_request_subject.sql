SET LOCAL lock_timeout = '3s';

CREATE TABLE IF NOT EXISTS api_request_subject (
  ts          timestamptz NOT NULL,
  request_id  uuid NOT NULL,
  member_id   bigint,
  actor_id    bigint,
  key_prefix  text,
  key_kind    smallint NOT NULL,
  service     smallint NOT NULL,
  status      smallint NOT NULL,
  form        smallint NOT NULL,
  player      uuid,
  name        text
) PARTITION BY RANGE (ts);

CREATE INDEX IF NOT EXISTS idx_ars_player ON api_request_subject (player, ts DESC)
  INCLUDE (member_id, actor_id, key_prefix, key_kind, service, status, form) WHERE player IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ars_name ON api_request_subject (name, ts DESC)
  INCLUDE (member_id, actor_id, key_prefix, key_kind, service, status, form) WHERE player IS NULL;
CREATE INDEX IF NOT EXISTS idx_ars_member ON api_request_subject (member_id, ts DESC)
  INCLUDE (player, name) WHERE member_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ars_actor ON api_request_subject (actor_id, ts DESC)
  INCLUDE (player, name) WHERE actor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ars_ts ON api_request_subject USING brin (ts);

DO $$
DECLARE d date := (now() AT TIME ZONE 'UTC')::date;
BEGIN
  FOR i IN 0..3 LOOP
    EXECUTE format(
      'CREATE TABLE IF NOT EXISTS api_request_subject_%s PARTITION OF api_request_subject FOR VALUES FROM (%L) TO (%L)',
      to_char(d + i, 'YYYYMMDD'), (d + i)::text || ' 00:00:00+00', (d + i + 1)::text || ' 00:00:00+00');
  END LOOP;
END $$;
