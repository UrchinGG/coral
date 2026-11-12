CREATE TABLE IF NOT EXISTS sync_sources (
    id bigserial PRIMARY KEY,
    name text NOT NULL UNIQUE,
    base_url text NOT NULL,
    api_key text,
    has_feed boolean NOT NULL,
    max_age_secs integer NOT NULL,
    last_seq bigint,
    created_by bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS guild_sync_sources (
    guild_id bigint NOT NULL REFERENCES guild_config (guild_id) ON DELETE CASCADE,
    source_id bigint NOT NULL REFERENCES sync_sources (id) ON DELETE CASCADE,
    PRIMARY KEY (guild_id, source_id)
);

-- value is NULL when the source answered that it does not know the member.
CREATE TABLE IF NOT EXISTS sync_source_values (
    source_id bigint NOT NULL REFERENCES sync_sources (id) ON DELETE CASCADE,
    discord_id bigint NOT NULL,
    value jsonb,
    fetched_at timestamptz NOT NULL,
    stale boolean NOT NULL DEFAULT false,
    PRIMARY KEY (source_id, discord_id)
);
