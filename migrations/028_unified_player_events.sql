-- Unify lock state into the event log. blacklist_players is gone.

ALTER TABLE player_tag_events RENAME TO player_events;

ALTER TABLE player_events ALTER COLUMN tag_type DROP NOT NULL;
ALTER TABLE player_events ADD COLUMN kind VARCHAR(16);
UPDATE player_events SET kind = CASE WHEN reason IS NULL THEN 'tag_clear' ELSE 'tag_set' END;
ALTER TABLE player_events ALTER COLUMN kind SET NOT NULL;
ALTER TABLE player_events ADD CONSTRAINT player_events_kind_check
    CHECK (kind IN ('tag_set', 'tag_clear', 'lock', 'unlock'));

-- Migrate active locks from blacklist_players (sparse — only locked rows matter).
INSERT INTO player_events (uuid, kind, reason, author, ts)
SELECT uuid, 'lock', lock_reason, locked_by, COALESCE(locked_at, NOW())
FROM blacklist_players
WHERE is_locked;

DROP TABLE player_tags_legacy;
DROP TABLE blacklist_players;

ALTER INDEX idx_pte_active RENAME TO idx_pe_active;
ALTER INDEX idx_pte_uuid_ts RENAME TO idx_pe_uuid_ts;
ALTER INDEX idx_pte_author RENAME TO idx_pe_author;
