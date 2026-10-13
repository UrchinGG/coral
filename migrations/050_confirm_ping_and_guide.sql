ALTER TABLE review_guide_config ADD COLUMN IF NOT EXISTS confirm_ping_role_id BIGINT;

WITH guide AS (
    SELECT
        E'If you don''t have direct-tag access yet, a **Blatant Cheater** or **Closet Cheater** tag you submit needs community approval before it''s applied.\n1. Run `/tag add` to open a private draft console that only you can see\n2. Add a replay, which is required, and any screenshots or clips as media\n3. Press **Submit** to publish your review for voting' AS submitting,
        E'Anyone with voting access, Reviewers and staff, can vote on a tag''s validity.\n**Accept**\n-# The tag is correct.\n**Reject**\n-# This player should not be tagged. No reason is needed.\n**Revise**\n-# The player should be tagged, but the type or reason is wrong. Give a reason, and it is posted in the thread without your name.\n1. 8 accepts with no rejects or revises approve the tag\n2. 3 rejects with no accepts reject it\n3. Staff can resolve any review themselves' AS voting
),
rewritten AS (
    SELECT
        CASE
            WHEN jsonb_typeof(config.content -> 'sections') = 'array' THEN jsonb_set(
                config.content,
                '{sections}',
                (
                    SELECT jsonb_agg(
                        CASE section ->> 'key'
                            WHEN 'submitting' THEN jsonb_set(section, '{body}', to_jsonb(guide.submitting))
                            WHEN 'voting' THEN jsonb_set(section, '{body}', to_jsonb(guide.voting))
                            ELSE section
                        END
                        ORDER BY position
                    )
                    FROM jsonb_array_elements(config.content -> 'sections') WITH ORDINALITY AS s(section, position)
                )
            )
            WHEN jsonb_typeof(config.content -> 'body') = 'string' THEN jsonb_set(
                config.content,
                '{body}',
                to_jsonb(
                    regexp_replace(
                        regexp_replace(
                            config.content ->> 'body',
                            E'(### Submitting\\n).*?(\\n---\\n)',
                            E'\\1' || guide.submitting || E'\\2'
                        ),
                        E'(### Voting\\n).*?(\\n---\\n)',
                        E'\\1' || guide.voting || E'\\2'
                    )
                )
            )
            ELSE config.content
        END AS content
    FROM review_guide_config config, guide
    WHERE config.id = 1
)
UPDATE review_guide_config
SET content = rewritten.content, content_updated_at = NOW()
FROM rewritten
WHERE review_guide_config.id = 1
  AND review_guide_config.content IS DISTINCT FROM rewritten.content;
