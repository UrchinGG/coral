CREATE TABLE IF NOT EXISTS minecraft_blocks (
    uuid uuid PRIMARY KEY,
    reason text,
    blocked_by bigint NOT NULL,
    blocked_at timestamptz NOT NULL DEFAULT now()
);
