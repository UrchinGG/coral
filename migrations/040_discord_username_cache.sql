CREATE TABLE IF NOT EXISTS discord_username_cache (
  discord_id      bigint PRIMARY KEY,
  username        text NOT NULL,
  last_refreshed  timestamptz NOT NULL DEFAULT now()
);
