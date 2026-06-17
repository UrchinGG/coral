CREATE TABLE IF NOT EXISTS guild_snapshots (
    id BIGSERIAL PRIMARY KEY,
    guild_id VARCHAR(24) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_baseline BOOLEAN NOT NULL DEFAULT FALSE,
    data JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_guild_snapshots_guild_ts
    ON guild_snapshots(guild_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_guild_snapshots_baseline
    ON guild_snapshots(guild_id, timestamp DESC) WHERE is_baseline;

CREATE TABLE IF NOT EXISTS guild_subscriptions (
    guild_id VARCHAR(24) NOT NULL,
    discord_id BIGINT NOT NULL,
    tag_types TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (guild_id, discord_id)
);

CREATE INDEX IF NOT EXISTS idx_guild_subscriptions_guild
    ON guild_subscriptions(guild_id);
