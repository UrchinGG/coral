-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_player_snapshots_discord_id
    ON player_snapshots (discord_id) WHERE discord_id IS NOT NULL;
