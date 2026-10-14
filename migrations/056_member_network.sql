SET LOCAL lock_timeout = '3s';

ALTER TABLE api_key_ips
  ADD COLUMN IF NOT EXISTS requests bigint NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS country text,
  ADD COLUMN IF NOT EXISTS proxied boolean NOT NULL DEFAULT false;

CREATE INDEX IF NOT EXISTS idx_api_key_ips_ip ON api_key_ips (ip_address);

CREATE TABLE IF NOT EXISTS api_member_agents (
  member_id  bigint NOT NULL REFERENCES members(id) ON DELETE CASCADE,
  user_agent text NOT NULL,
  agent_md5  uuid GENERATED ALWAYS AS (md5(user_agent)::uuid) STORED,
  first_seen timestamptz NOT NULL,
  last_seen  timestamptz NOT NULL,
  requests   bigint NOT NULL,
  CONSTRAINT api_member_agents_key UNIQUE (member_id, agent_md5)
);
