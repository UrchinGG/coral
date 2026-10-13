CREATE TABLE IF NOT EXISTS review (
    id BIGSERIAL PRIMARY KEY,
    player_uuid VARCHAR(32) NOT NULL,
    submitter_id BIGINT NOT NULL,
    hide_username BOOLEAN NOT NULL DEFAULT FALSE,
    thread_id BIGINT,
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'open', 'accepted', 'rejected', 'voided', 'expired')),
    staff_resolved BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_by BIGINT,
    resolved_at TIMESTAMPTZ,
    voided_by BIGINT,
    voided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_review_open_player
    ON review (player_uuid) WHERE status = 'open';
CREATE UNIQUE INDEX IF NOT EXISTS idx_review_draft_player_submitter
    ON review (player_uuid, submitter_id) WHERE status = 'draft';
CREATE UNIQUE INDEX IF NOT EXISTS idx_review_thread
    ON review (thread_id) WHERE thread_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_review_submitter ON review (submitter_id);

CREATE TABLE IF NOT EXISTS review_revision (
    review_id BIGINT NOT NULL REFERENCES review(id) ON DELETE CASCADE,
    revision INT NOT NULL,
    reason TEXT NOT NULL,
    tag_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    PRIMARY KEY (review_id, revision)
);

CREATE INDEX IF NOT EXISTS idx_review_revision_unpublished
    ON review_revision (review_id) WHERE published_at IS NULL;

CREATE TABLE IF NOT EXISTS review_evidence (
    id BIGSERIAL PRIMARY KEY,
    review_id BIGINT NOT NULL,
    revision INT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('replay', 'media')),
    replay_id TEXT,
    replay_ts TEXT,
    filename TEXT,
    content_type TEXT,
    attachment_message_id BIGINT,
    attachment_id BIGINT,
    staged_bytes BYTEA,
    note TEXT,
    added_by BIGINT NOT NULL,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    position INT NOT NULL,
    FOREIGN KEY (review_id, revision) REFERENCES review_revision (review_id, revision)
        ON DELETE CASCADE,
    CHECK ((kind = 'replay') = (replay_id IS NOT NULL)),
    CHECK ((kind = 'media') = (filename IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS idx_review_evidence_revision
    ON review_evidence (review_id, revision, position);

CREATE TABLE IF NOT EXISTS review_vote (
    id BIGSERIAL PRIMARY KEY,
    review_id BIGINT NOT NULL REFERENCES review(id) ON DELETE CASCADE,
    voter_id BIGINT NOT NULL,
    revision INT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('accept', 'reject', 'revise')),
    reason TEXT,
    disposition TEXT NOT NULL DEFAULT 'active'
        CHECK (disposition IN ('active', 'superseded', 'voided')),
    cast_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK ((kind = 'revise') = (reason IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_review_vote_active
    ON review_vote (review_id, voter_id) WHERE disposition = 'active';
CREATE INDEX IF NOT EXISTS idx_review_vote_voter ON review_vote (voter_id);
