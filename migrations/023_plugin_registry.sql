ALTER TABLE starfish_users
    ADD COLUMN github_user_id   BIGINT,
    ADD COLUMN github_username  TEXT,
    ADD COLUMN github_linked_at TIMESTAMPTZ;

CREATE UNIQUE INDEX idx_starfish_users_github_user_id
    ON starfish_users(github_user_id)
    WHERE github_user_id IS NOT NULL;


CREATE TABLE plugins (
    id              BIGSERIAL PRIMARY KEY,
    slug            TEXT NOT NULL UNIQUE,
    owner_user_id   BIGINT NOT NULL REFERENCES starfish_users(id),
    repo            TEXT NOT NULL,
    github_repo_id  BIGINT NOT NULL UNIQUE,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    tags            TEXT[] NOT NULL DEFAULT '{}',
    license         TEXT NOT NULL DEFAULT 'MIT',
    homepage        TEXT,
    official        BOOLEAN NOT NULL DEFAULT false,
    disabled        BOOLEAN NOT NULL DEFAULT false,
    disabled_reason TEXT,
    disabled_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (slug ~ '^[a-z][a-z0-9-]{2,63}$')
);

CREATE INDEX idx_plugins_tags ON plugins USING GIN(tags);
CREATE INDEX idx_plugins_owner ON plugins(owner_user_id);
CREATE INDEX idx_plugins_disabled ON plugins(disabled_at) WHERE disabled;


CREATE TABLE plugin_releases (
    id              BIGSERIAL PRIMARY KEY,
    plugin_id       BIGINT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    version         TEXT NOT NULL,
    git_sha         TEXT NOT NULL,
    asset_url       TEXT NOT NULL,
    asset_sha256    BYTEA NOT NULL,
    asset_size      INTEGER NOT NULL,
    body_cache      BYTEA,
    readme_cache    TEXT,
    manifest_json   JSONB NOT NULL,
    changelog       TEXT,
    yanked          BOOLEAN NOT NULL DEFAULT false,
    yanked_at       TIMESTAMPTZ,
    yanked_reason   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (plugin_id, version),
    CHECK (version ~ '^[0-9]+\.[0-9]+\.[0-9]+$')
);

CREATE INDEX idx_plugin_releases_plugin_created
    ON plugin_releases(plugin_id, created_at DESC)
    WHERE NOT yanked;


CREATE TABLE plugin_installs (
    user_id         BIGINT NOT NULL REFERENCES starfish_users(id) ON DELETE CASCADE,
    plugin_id       BIGINT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    release_id      BIGINT NOT NULL REFERENCES plugin_releases(id),
    installed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, plugin_id)
);

CREATE INDEX idx_plugin_installs_plugin_installed_at
    ON plugin_installs(plugin_id, installed_at);


CREATE TABLE plugin_ratings (
    user_id    BIGINT NOT NULL REFERENCES starfish_users(id) ON DELETE CASCADE,
    plugin_id  BIGINT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    stars      SMALLINT NOT NULL CHECK (stars BETWEEN 1 AND 5),
    review     TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, plugin_id)
);

CREATE INDEX idx_plugin_ratings_plugin ON plugin_ratings(plugin_id);


CREATE TABLE plugin_sort_config (
    id                      INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    velocity_weight         REAL NOT NULL DEFAULT 0.50,
    rating_weight           REAL NOT NULL DEFAULT 0.30,
    recency_weight          REAL NOT NULL DEFAULT 0.20,
    rating_prior_confidence REAL NOT NULL DEFAULT 5.0,
    rating_prior_mean       REAL NOT NULL DEFAULT 3.5,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO plugin_sort_config (id) VALUES (1);
