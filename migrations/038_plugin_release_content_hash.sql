DO $$ BEGIN
    ALTER TABLE plugin_releases ADD COLUMN content_sha256 BYTEA;
EXCEPTION WHEN duplicate_column THEN NULL;
END $$;
