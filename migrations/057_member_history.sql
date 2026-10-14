SET LOCAL lock_timeout = '3s';

CREATE TABLE IF NOT EXISTS member_api_keys (
  id         bigserial PRIMARY KEY,
  member_id  bigint NOT NULL REFERENCES members(id) ON DELETE CASCADE,
  kind       text NOT NULL,
  api_key    text NOT NULL UNIQUE,
  created_at timestamptz,
  retired_at timestamptz
);

CREATE INDEX IF NOT EXISTS idx_member_api_keys_member ON member_api_keys (member_id);
CREATE INDEX IF NOT EXISTS idx_member_api_keys_prefix ON member_api_keys (left(api_key, 8));

INSERT INTO member_api_keys (member_id, kind, api_key)
  SELECT id, 'personal', api_key FROM members WHERE api_key IS NOT NULL
  ON CONFLICT DO NOTHING;
INSERT INTO member_api_keys (member_id, kind, api_key, created_at)
  SELECT member_id, 'developer', api_key, created_at FROM developer_keys
  ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS member_mc_links (
  id          bigserial PRIMARY KEY,
  member_id   bigint NOT NULL REFERENCES members(id) ON DELETE CASCADE,
  uuid        varchar(32) NOT NULL,
  relation    text NOT NULL,
  linked_at   timestamptz,
  unlinked_at timestamptz
);

CREATE INDEX IF NOT EXISTS idx_member_mc_links_member ON member_mc_links (member_id);
CREATE INDEX IF NOT EXISTS idx_member_mc_links_uuid ON member_mc_links (uuid);
CREATE UNIQUE INDEX IF NOT EXISTS idx_member_mc_links_open ON member_mc_links (member_id, uuid, relation) WHERE unlinked_at IS NULL;

INSERT INTO member_mc_links (member_id, uuid, relation)
  SELECT id, uuid, 'primary' FROM members WHERE uuid IS NOT NULL
  ON CONFLICT (member_id, uuid, relation) WHERE unlinked_at IS NULL DO NOTHING;
INSERT INTO member_mc_links (member_id, uuid, relation, linked_at)
  SELECT member_id, uuid, 'alt', added_at FROM minecraft_accounts
  ON CONFLICT (member_id, uuid, relation) WHERE unlinked_at IS NULL DO NOTHING;
