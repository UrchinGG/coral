CREATE TABLE IF NOT EXISTS member_strike (
    id BIGSERIAL PRIMARY KEY,
    member_id BIGINT NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    struck_by BIGINT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_by BIGINT,
    source_ref TEXT
);

CREATE INDEX IF NOT EXISTS idx_member_strike_member ON member_strike (member_id, issued_at DESC);
CREATE INDEX IF NOT EXISTS idx_member_strike_live_expiry
    ON member_strike (expires_at) WHERE revoked_at IS NULL AND expires_at IS NOT NULL;
