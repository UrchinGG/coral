DROP INDEX IF EXISTS idx_plugins_tags;

DO $$ BEGIN
    ALTER TABLE plugins DROP COLUMN tags;
EXCEPTION WHEN undefined_column THEN NULL;
END $$;
