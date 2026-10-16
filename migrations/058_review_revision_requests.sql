SET LOCAL lock_timeout = '3s';

CREATE TABLE IF NOT EXISTS review_revision_request (
  id          bigserial PRIMARY KEY,
  review_id   bigint NOT NULL REFERENCES review(id),
  author_id   bigint NOT NULL,
  tag_type    text NOT NULL,
  reason      text NOT NULL,
  message_id  bigint,
  status      text NOT NULL DEFAULT 'open',
  applied_by  bigint,
  created_at  timestamptz NOT NULL DEFAULT now(),
  resolved_at timestamptz
);

CREATE INDEX IF NOT EXISTS idx_review_revision_request_review_status
  ON review_revision_request (review_id, status);

CREATE TABLE IF NOT EXISTS review_revision_request_vote (
  request_id bigint NOT NULL REFERENCES review_revision_request(id) ON DELETE CASCADE,
  voter_id   bigint NOT NULL,
  yes        boolean NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (request_id, voter_id)
);
