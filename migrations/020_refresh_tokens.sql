CREATE TABLE starfish_refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES starfish_users(id) ON DELETE CASCADE,
    hwid_id BIGINT NOT NULL REFERENCES starfish_hwids(id) ON DELETE CASCADE,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_starfish_refresh_tokens_user ON starfish_refresh_tokens(user_id);
CREATE INDEX idx_starfish_refresh_tokens_hash ON starfish_refresh_tokens(token_hash);
