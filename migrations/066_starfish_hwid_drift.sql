-- Small hardware changes on the same machine (a near-match HWID) are logged here
-- too, but they don't count toward the HWID change limit.
ALTER TABLE starfish_hwid_changes ADD COLUMN IF NOT EXISTS is_drift BOOLEAN NOT NULL DEFAULT false;
