CREATE TABLE IF NOT EXISTS guild_sync_jobs (
    id BIGSERIAL PRIMARY KEY,
    guild_id BIGINT NOT NULL,
    kind TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'queued',
    processed INT NOT NULL DEFAULT 0,
    total INT,
    cancel_requested BOOLEAN NOT NULL DEFAULT false,
    error TEXT,
    created_by BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_guild_sync_jobs_guild_status ON guild_sync_jobs(guild_id, status);
CREATE INDEX IF NOT EXISTS idx_guild_sync_jobs_status ON guild_sync_jobs(status);

CREATE TABLE IF NOT EXISTS review_guide_config (
    id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    content JSONB NOT NULL,
    review_ping_role_id BIGINT,
    dispute_ping_role_id BIGINT,
    posted_channel_id BIGINT,
    posted_thread_id BIGINT,
    posted_message_id BIGINT,
    posted_at TIMESTAMPTZ,
    posted_by BIGINT,
    content_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO review_guide_config (id, content) VALUES (1, '{
  "title": "Tag Review Guide",
  "tags": [
    {
      "key": "sniper",
      "name": "Sniper",
      "emoji": "<:sniper:1459106167270932618>",
      "description": "Used for cheating snipers. Check the tooltip date; if it''s old, they may no longer be active."
    },
    {
      "key": "blatant_cheater",
      "name": "Blatant Cheater",
      "emoji": "<:blatantcheater:1459106183196577812>",
      "description": "Obvious cheats that would be impossible on a vanilla client, like scaffold, speedmine, or autoblock."
    },
    {
      "key": "closet_cheater",
      "name": "Closet Cheater",
      "emoji": "<:closetcheater:1459106337039323136>",
      "description": "Cheats that can be more subtle, like legit scaffold, aimassist, or lagrange."
    },
    {
      "key": "confirmed_cheater",
      "name": "Confirmed Cheater",
      "emoji": "<:confirmedcheater:1459106129765204049>",
      "description": "Applied to players that have been confirmed to be cheating by staff. Typically, video evidence is available for these players on request."
    },
    {
      "key": "replays_needed",
      "name": "Replays Needed",
      "emoji": "<:replaysneeded:1482502914835615745>",
      "description": "Used whenever staff require replays of a player for any reason. Remember to submit replays to staff, it helps us prove players legit and clear their tags."
    },
    {
      "key": "caution",
      "name": "Caution",
      "emoji": "<:caution:1459106358098923583>",
      "description": "Special tag used for things that don''t fit into any of the above categories. Only staff can apply this."
    }
  ],
  "sections": [
    {
      "key": "submitting",
      "heading": "Submitting",
      "body": "If you don''t have direct-tag access yet, a **Blatant Cheater** or **Closet Cheater** tag you submit needs community approval before it''s applied.\n1. Run `/tag add`, then press **Create Post** on the preview\n2. Attach proof with the **+ Replay** and **+ Media** buttons in your new post\n3. Press **Submit** once your evidence is ready for review"
    },
    {
      "key": "voting",
      "heading": "Voting",
      "body": "Anyone with voting access, Reviewers and staff, can vote **Accept** or **Reject** on a tag''s validity.\n1. If votes stay unanimous, the tag resolves automatically once enough come in\n2. If votes disagree, review will not resolve automatically and a moderator steps in to make the final call\n-# Explain your reasoning when you reject a tag, it helps the submitter understand what to fix."
    },
    {
      "key": "standing",
      "heading": "Standing",
      "body": "**Default**\n-# **Blatant Cheater** and **Closet Cheater** tags you submit go through review. Sniper tags still apply directly, no review needed. A number of approved submissions with no rejections unlock voting.\n**Reviewer**\n-# You can vote on reviews. Accurate verdicts progress you toward Trusted.\n**Trusted**\n-# You can tag players directly, skipping review.\nRun `/dashboard` to track your standing and progress."
    }
  ],
  "footer": "Press the button below to toggle tag review pings if you''d like to be alerted when new ones are submitted."
}'::jsonb)
ON CONFLICT (id) DO NOTHING;
