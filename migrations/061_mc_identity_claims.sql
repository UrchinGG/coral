SET LOCAL lock_timeout = '3s';

CREATE TABLE IF NOT EXISTS mc_identity_claims (
  uuid       varchar(32) PRIMARY KEY,
  member_id  bigint NOT NULL REFERENCES members(id) ON DELETE CASCADE,
  claimed_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_mc_identity_claims_member ON mc_identity_claims (member_id);
