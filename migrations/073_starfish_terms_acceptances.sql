CREATE TABLE starfish_terms_acceptances (
    discord_id BIGINT NOT NULL,
    terms_version TEXT NOT NULL,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (discord_id, terms_version)
);
