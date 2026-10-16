SET LOCAL lock_timeout = '3s';

CREATE TABLE IF NOT EXISTS review_evidence_prompt (
  source_message_id bigint PRIMARY KEY,
  review_id         bigint NOT NULL REFERENCES review(id),
  prompt_message_id bigint NOT NULL,
  author_id         bigint NOT NULL,
  status            text NOT NULL DEFAULT 'open',
  created_at        timestamptz NOT NULL DEFAULT now(),
  resolved_at       timestamptz
);

CREATE INDEX IF NOT EXISTS idx_review_evidence_prompt_review_status
  ON review_evidence_prompt (review_id, status);

CREATE TABLE IF NOT EXISTS review_evidence_prompt_vote (
  source_message_id bigint NOT NULL
    REFERENCES review_evidence_prompt(source_message_id) ON DELETE CASCADE,
  voter_id          bigint NOT NULL,
  add               boolean NOT NULL,
  created_at        timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (source_message_id, voter_id)
);
