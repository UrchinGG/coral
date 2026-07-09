ALTER TABLE members ADD COLUMN incorrect_verdicts BIGINT NOT NULL DEFAULT 0;
ALTER TABLE members ADD COLUMN bonus_verdicts BIGINT NOT NULL DEFAULT 0;
ALTER TABLE members ADD COLUMN vote_granted BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE members ADD COLUMN tag_granted BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE members ADD COLUMN strikes JSONB NOT NULL DEFAULT '[]'::jsonb;
UPDATE members SET strikes = config -> 'strikes' WHERE config ? 'strikes';
UPDATE members SET config = config - 'strikes' WHERE config ? 'strikes';
