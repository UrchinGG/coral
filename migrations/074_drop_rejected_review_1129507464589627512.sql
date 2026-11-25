-- One-off score correction: drop a single rejected tag review from member
-- 1129507464589627512. Mirrors the "void_rejected" correction in the bot's
-- Tag Reviews panel, including its admin_actions audit row, and is a no-op
-- when the member is missing or has no rejections left.
WITH before AS (
    SELECT discord_id, accepted_tags, rejected_tags, accurate_verdicts,
           incorrect_verdicts, bonus_verdicts
    FROM members
    WHERE discord_id = 1129507464589627512 AND rejected_tags > 0
    FOR UPDATE
),
updated AS (
    UPDATE members m
    SET rejected_tags = m.rejected_tags - 1
    FROM before b
    WHERE m.discord_id = b.discord_id
    RETURNING m.discord_id, m.accepted_tags, m.rejected_tags, m.accurate_verdicts,
              m.incorrect_verdicts, m.bonus_verdicts
)
INSERT INTO admin_actions (actor, action, target, details)
SELECT
    u.discord_id,
    'review_score_correction',
    u.discord_id::text,
    jsonb_build_object(
        'correction', 'void_rejected',
        'label', 'Drop a rejected review',
        'count', 1,
        'reason', 'Applied by migration 074',
        'before', jsonb_build_object(
            'accepted_tags', b.accepted_tags,
            'rejected_tags', b.rejected_tags,
            'accurate_verdicts', b.accurate_verdicts,
            'incorrect_verdicts', b.incorrect_verdicts,
            'bonus_verdicts', b.bonus_verdicts
        ),
        'after', jsonb_build_object(
            'accepted_tags', u.accepted_tags,
            'rejected_tags', u.rejected_tags,
            'accurate_verdicts', u.accurate_verdicts,
            'incorrect_verdicts', u.incorrect_verdicts,
            'bonus_verdicts', u.bonus_verdicts
        )
    )
FROM updated u
JOIN before b ON b.discord_id = u.discord_id;
