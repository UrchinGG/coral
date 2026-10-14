SET LOCAL lock_timeout = '3s';

CREATE TABLE IF NOT EXISTS api_traffic_minute (
  minute          timestamptz NOT NULL,
  route           text,
  method          text NOT NULL,
  status          smallint NOT NULL,
  cause           smallint NOT NULL,
  key_kind        smallint NOT NULL,
  requests        integer NOT NULL,
  latency_sum_ms  bigint NOT NULL,
  latency_max_ms  integer NOT NULL,
  latency_buckets integer[] NOT NULL,
  CONSTRAINT api_traffic_minute_key UNIQUE NULLS NOT DISTINCT (minute, route, method, status, cause, key_kind)
) WITH (fillfactor = 80);

CREATE TABLE IF NOT EXISTS api_traffic_hour (
  hour            timestamptz NOT NULL,
  route           text,
  method          text NOT NULL,
  status          smallint NOT NULL,
  cause           smallint NOT NULL,
  key_kind        smallint NOT NULL,
  requests        integer NOT NULL,
  latency_sum_ms  bigint NOT NULL,
  latency_max_ms  integer NOT NULL,
  latency_buckets integer[] NOT NULL,
  CONSTRAINT api_traffic_hour_key UNIQUE NULLS NOT DISTINCT (hour, route, method, status, cause, key_kind)
) WITH (fillfactor = 80);

CREATE TABLE IF NOT EXISTS api_caller_hour (
  hour            timestamptz NOT NULL,
  key_kind        smallint NOT NULL,
  member_id       bigint,
  actor_id        bigint,
  key_prefix      text,
  service         smallint NOT NULL,
  route           text,
  method          text NOT NULL,
  status          smallint NOT NULL,
  cause           smallint NOT NULL,
  requests        integer NOT NULL,
  latency_sum_ms  bigint NOT NULL,
  subjects        bigint NOT NULL,
  CONSTRAINT api_caller_hour_key UNIQUE NULLS NOT DISTINCT
    (hour, key_kind, member_id, actor_id, key_prefix, service, route, method, status, cause)
) WITH (fillfactor = 80);

CREATE INDEX IF NOT EXISTS idx_ach_member ON api_caller_hour (member_id, hour DESC) WHERE member_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ach_actor ON api_caller_hour (actor_id, hour DESC) WHERE actor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_ach_prefix ON api_caller_hour (key_prefix, hour DESC) WHERE key_prefix IS NOT NULL;

CREATE TABLE IF NOT EXISTS api_rollup_state (
  name      text PRIMARY KEY,
  watermark timestamptz NOT NULL
);

CREATE TABLE IF NOT EXISTS api_rollup_dirty (
  id         bigserial PRIMARY KEY,
  from_ts    timestamptz NOT NULL,
  to_ts      timestamptz NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS api_coverage (
  feature text PRIMARY KEY,
  since   timestamptz NOT NULL,
  note    text
);

CREATE TABLE IF NOT EXISTS api_coverage_gap (
  feature text NOT NULL,
  from_ts timestamptz NOT NULL,
  to_ts   timestamptz NOT NULL,
  PRIMARY KEY (feature, from_ts)
);

INSERT INTO api_coverage (feature, since, note) VALUES
  ('detail', '2026-10-14 03:14:32+00', 'detail logging began'),
  ('body_subjects', '2026-10-14 03:14:32+00', 'batch bodies before this were never stored')
ON CONFLICT DO NOTHING;
