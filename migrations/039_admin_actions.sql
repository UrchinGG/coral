CREATE TABLE IF NOT EXISTS admin_actions (
  id          bigserial PRIMARY KEY,
  actor       bigint NOT NULL,
  action      text NOT NULL,
  target      text NOT NULL,
  details     jsonb NOT NULL DEFAULT '{}'::jsonb,
  ts          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_admin_actions_ts ON admin_actions (ts DESC);
CREATE INDEX IF NOT EXISTS idx_admin_actions_actor ON admin_actions (actor, ts DESC);
CREATE INDEX IF NOT EXISTS idx_admin_actions_target ON admin_actions (target, ts DESC);
