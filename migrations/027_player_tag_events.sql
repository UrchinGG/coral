-- Append-only event log for tag mutations.
-- Each row is one event. reason IS NULL means this event ended the tag of that type.
-- An overwrite is two rows with the same author and ts: one ending the old type, one starting the new.
-- author IS NULL means the system (expiry sweep, bulk import).

CREATE TABLE IF NOT EXISTS player_tag_events (
    id BIGSERIAL PRIMARY KEY,
    uuid VARCHAR(32) NOT NULL,
    tag_type VARCHAR(32) NOT NULL,
    reason TEXT,
    hide_username BOOLEAN,
    expires_at TIMESTAMPTZ,
    reviewed_by BIGINT[],
    author BIGINT,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pte_active ON player_tag_events (uuid, tag_type, ts DESC);
CREATE INDEX IF NOT EXISTS idx_pte_uuid_ts ON player_tag_events (uuid, ts DESC);
CREATE INDEX IF NOT EXISTS idx_pte_author ON player_tag_events (author) WHERE author IS NOT NULL;

-- Backfill: emit an "add" event per existing player_tags row.
INSERT INTO player_tag_events (uuid, tag_type, reason, hide_username, expires_at, reviewed_by, author, ts)
SELECT bp.uuid, pt.tag_type, pt.reason, pt.hide_username, pt.expires_at, pt.reviewed_by, pt.added_by, pt.added_on
FROM player_tags pt
JOIN blacklist_players bp ON pt.player_id = bp.id;

-- Backfill: emit a "remove" event for each removed row.
INSERT INTO player_tag_events (uuid, tag_type, reason, author, ts)
SELECT bp.uuid, pt.tag_type, NULL, pt.removed_by, pt.removed_on
FROM player_tags pt
JOIN blacklist_players bp ON pt.player_id = bp.id
WHERE pt.removed_on IS NOT NULL;

-- Retain the old table as legacy for one release cycle. Drop in a follow-up migration.
ALTER TABLE player_tags RENAME TO player_tags_legacy;
