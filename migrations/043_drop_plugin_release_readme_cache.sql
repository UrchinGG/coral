DO $$ BEGIN
    ALTER TABLE plugin_releases DROP COLUMN readme_cache;
EXCEPTION WHEN undefined_column THEN NULL;
END $$;
