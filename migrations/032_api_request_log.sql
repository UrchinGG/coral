CREATE TABLE IF NOT EXISTS api_request_log (
  id          bigserial,
  ts          timestamptz NOT NULL DEFAULT now(),
  member_id   bigint,
  key_kind    text,
  key_prefix  text,
  ip          text,
  method      text,
  path        text,
  status      smallint,
  latency_ms  integer
) PARTITION BY RANGE (ts);

CREATE INDEX IF NOT EXISTS idx_arl_ts ON api_request_log (ts DESC);
CREATE INDEX IF NOT EXISTS idx_arl_member ON api_request_log (member_id, ts DESC);
CREATE INDEX IF NOT EXISTS idx_arl_prefix ON api_request_log (key_prefix, ts DESC);
CREATE INDEX IF NOT EXISTS idx_arl_status ON api_request_log (status, ts DESC);
CREATE INDEX IF NOT EXISTS idx_arl_path ON api_request_log (path, ts DESC);
CREATE INDEX IF NOT EXISTS idx_arl_ip ON api_request_log (ip, ts DESC);
