ALTER TABLE api_request_log
  ADD COLUMN IF NOT EXISTS query      text,
  ADD COLUMN IF NOT EXISTS user_agent text,
  ADD COLUMN IF NOT EXISTS error      text;
