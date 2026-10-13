CREATE TABLE IF NOT EXISTS review_rules (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    accept_threshold BIGINT NOT NULL DEFAULT 8,
    reject_threshold BIGINT NOT NULL DEFAULT 3,
    vote_grant_approvals BIGINT NOT NULL DEFAULT 5,
    tag_grant_correct BIGINT NOT NULL DEFAULT 25,
    earn_ratio BIGINT NOT NULL DEFAULT 12,
    bonus_weight BIGINT NOT NULL DEFAULT 5,
    revoke_strikes BIGINT NOT NULL DEFAULT 2,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by BIGINT
);

INSERT INTO review_rules (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
