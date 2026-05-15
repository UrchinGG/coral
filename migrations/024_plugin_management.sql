ALTER TABLE plugins
    ADD COLUMN page_override TEXT,
    ADD COLUMN unlisted      BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN unlisted_at   TIMESTAMPTZ;

CREATE INDEX idx_plugins_unlisted ON plugins(unlisted_at) WHERE unlisted;
