-- Lets staff pin a member to a standing tier instead of letting the automatic
-- rules decide. Kept separate from vote_granted/tag_granted so those stay the
-- pure automatic hysteresis state and clearing an override drops the member
-- back exactly where the automatic system had them.
ALTER TABLE members ADD COLUMN standing_override TEXT
    CHECK (standing_override IN ('restricted', 'submitter', 'reviewer', 'trusted'));
ALTER TABLE members ADD COLUMN standing_override_by BIGINT;
ALTER TABLE members ADD COLUMN standing_override_at TIMESTAMPTZ;
