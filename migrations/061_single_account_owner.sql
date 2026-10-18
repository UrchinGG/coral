SET LOCAL lock_timeout = '3s';

CREATE UNIQUE INDEX IF NOT EXISTS uniq_members_uuid ON members (uuid) WHERE uuid IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uniq_minecraft_accounts_uuid ON minecraft_accounts (uuid);
