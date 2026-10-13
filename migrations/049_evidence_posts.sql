CREATE TABLE IF NOT EXISTS evidence_post (
    thread_id BIGINT PRIMARY KEY,
    uuid VARCHAR(32) NOT NULL,
    origin TEXT NOT NULL CHECK (origin IN ('coral', 'legacy')),
    status TEXT CHECK (status IN ('pending', 'confirmed', 'archived')),
    title TEXT NOT NULL,
    pending_tag_type TEXT,
    pending_reason TEXT,
    pending_by BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK ((origin = 'coral') = (status IS NOT NULL)),
    CHECK ((pending_tag_type IS NULL) = (pending_reason IS NULL)
       AND (pending_tag_type IS NULL) = (pending_by IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_evidence_post_uuid ON evidence_post (uuid, origin, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_evidence_post_status ON evidence_post (status) WHERE status IS NOT NULL;
