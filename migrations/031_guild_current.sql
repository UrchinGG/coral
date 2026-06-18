DROP TABLE IF EXISTS guild_lookups;

CREATE TABLE guild_current (
    guild_id     TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    tag          TEXT,
    tag_color    TEXT,
    level        INTEGER NOT NULL,
    experience   BIGINT NOT NULL,
    member_count INTEGER NOT NULL,
    created      BIGINT,
    raw          JSONB NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_guild_current_name ON guild_current (LOWER(name));
CREATE INDEX idx_guild_current_experience ON guild_current (experience DESC);
CREATE INDEX idx_guild_current_level ON guild_current (level DESC);
CREATE INDEX idx_guild_current_raw ON guild_current USING gin (raw jsonb_path_ops);
