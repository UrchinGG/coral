-- Sync can be turned off for a server without losing its settings.
ALTER TABLE guild_config ADD COLUMN IF NOT EXISTS enabled boolean NOT NULL DEFAULT true;
