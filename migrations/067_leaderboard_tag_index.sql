CREATE INDEX IF NOT EXISTS idx_pe_tag_set_ts ON player_events (ts, author) WHERE kind = 'tag_set';
