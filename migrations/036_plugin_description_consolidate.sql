UPDATE plugins
SET description = page_override
WHERE page_override IS NOT NULL AND page_override <> '';

ALTER TABLE plugins DROP COLUMN page_override;
