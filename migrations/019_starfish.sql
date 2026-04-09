CREATE TABLE starfish_users (
    id BIGSERIAL PRIMARY KEY,
    discord_id BIGINT NOT NULL UNIQUE,
    license_status VARCHAR(16) NOT NULL DEFAULT 'inactive'
        CHECK (license_status IN ('inactive', 'active', 'suspended')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE starfish_hwids (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES starfish_users(id) ON DELETE CASCADE,
    hwid_hash VARCHAR(64) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, hwid_hash)
);

CREATE INDEX idx_starfish_hwids_user ON starfish_hwids(user_id);

CREATE TABLE starfish_hwid_changes (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES starfish_users(id) ON DELETE CASCADE,
    old_hwid VARCHAR(64),
    new_hwid VARCHAR(64) NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_starfish_hwid_changes_user ON starfish_hwid_changes(user_id);

CREATE TABLE starfish_sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES starfish_users(id) ON DELETE CASCADE,
    hwid_id BIGINT NOT NULL REFERENCES starfish_hwids(id) ON DELETE CASCADE,
    session_token VARCHAR(128) NOT NULL UNIQUE,
    key_material BYTEA NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signature BYTEA NOT NULL
);

CREATE INDEX idx_starfish_sessions_token ON starfish_sessions(session_token);
CREATE INDEX idx_starfish_sessions_expires_at ON starfish_sessions(expires_at);

CREATE TABLE starfish_device_codes (
    id BIGSERIAL PRIMARY KEY,
    device_code VARCHAR(128) NOT NULL UNIQUE,
    user_code VARCHAR(32) NOT NULL,
    client_hwid VARCHAR(64) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_starfish_device_codes_expires_at ON starfish_device_codes(expires_at);
CREATE INDEX idx_starfish_users_discord_id ON starfish_users(discord_id);
