-- Templates and rules written before Hypixel moved under `hypixel.*` are version 1;
-- coral-sync rewrites them once. Rows created from now on are already version 2.
ALTER TABLE guild_config ADD COLUMN IF NOT EXISTS context_version smallint NOT NULL DEFAULT 1;
ALTER TABLE guild_config ALTER COLUMN context_version SET DEFAULT 2;
