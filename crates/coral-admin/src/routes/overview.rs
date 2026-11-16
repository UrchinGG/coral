use std::fmt::Display;

use axum::extract::State;
use database::{
    BlacklistRepository, CacheRepository, GuildCacheRepository, GuildCurrentRepository,
    MemberRepository,
};
use serde::Serialize;
use tracing::warn;

use crate::error::AdminError;
use crate::state::AppState;
use crate::views::{CachedView, View};

const OVERVIEW_VIEW: &str = "overview";

#[derive(Serialize)]
struct Overview {
    bot: Option<BotInstalls>,
    members: Members,
    blacklist: Blacklist,
    players: Cached,
    guilds: Cached,
    failures: Vec<&'static str>,
}

#[derive(Serialize)]
struct BotInstalls {
    servers: u64,
    users: u64,
}

#[derive(Serialize)]
struct Members {
    registered: Option<i64>,
    trusted: Option<i64>,
    lifetime_requests: Option<i64>,
}

#[derive(Serialize)]
struct Blacklist {
    active: Option<i64>,
    by_type: Vec<TagCount>,
}

#[derive(Serialize)]
struct TagCount {
    tag_type: String,
    count: i64,
}

#[derive(Serialize)]
struct Cached {
    tracked: Option<i64>,
    snapshots: Option<i64>,
    storage_bytes: Option<i64>,
}

/// Collects each count on its own, so one failing query blanks only its figure.
#[derive(Default)]
struct Failures(Vec<&'static str>);

pub async fn overview(State(state): State<AppState>) -> Result<CachedView, AdminError> {
    state
        .views
        .get_or_load(View::Overview, OVERVIEW_VIEW, || load(&state))
        .await
}

async fn load(state: &AppState) -> Result<Overview, AdminError> {
    let pool = state.db.pool();
    let members = MemberRepository::new(pool);
    let blacklist = BlacklistRepository::new(pool);
    let players = CacheRepository::new(pool);
    let guild_snapshots = GuildCacheRepository::new(pool);
    let guilds = GuildCurrentRepository::new(pool);
    let (
        (registered, trusted, requests, active, by_type),
        (player_count, snapshots, storage, guild_count, guild_snaps, guild_storage),
        bot,
    ) = tokio::join!(
        async {
            tokio::join!(
                members.count(),
                members.count_trusted(),
                members.total_requests(),
                blacklist.count_active_tags(),
                blacklist.count_active_tags_by_type(),
            )
        },
        async {
            tokio::join!(
                players.count_unique_players(),
                players.count_snapshots(),
                players.storage_bytes(),
                guilds.count_guilds(),
                guild_snapshots.count_snapshots(),
                guild_snapshots.storage_bytes(),
            )
        },
        bot_installs(state),
    );
    let mut failed = Failures::default();
    Ok(Overview {
        bot,
        members: Members {
            registered: failed.read("registered members", registered),
            trusted: failed.read("trusted members", trusted),
            lifetime_requests: failed.read("lifetime requests", requests),
        },
        blacklist: Blacklist {
            active: failed.read("active tags", active),
            by_type: failed
                .read("tags by type", by_type)
                .unwrap_or_default()
                .into_iter()
                .map(|(tag_type, count)| TagCount { tag_type, count })
                .collect(),
        },
        players: Cached {
            tracked: failed.read("tracked players", player_count),
            snapshots: failed.read("player snapshots", snapshots),
            storage_bytes: failed.read("player cache size", storage),
        },
        guilds: Cached {
            tracked: failed.read("tracked guilds", guild_count),
            snapshots: failed.read("guild snapshots", guild_snaps),
            storage_bytes: failed.read("guild cache size", guild_storage),
        },
        failures: failed.0,
    })
}

impl Failures {
    fn read<T, E: Display>(&mut self, figure: &'static str, outcome: Result<T, E>) -> Option<T> {
        outcome
            .inspect_err(|error| {
                warn!(%error, figure, "count an overview figure");
                self.0.push(figure);
            })
            .ok()
    }
}

async fn bot_installs(state: &AppState) -> Option<BotInstalls> {
    let http = state.discord.bot().ok()?;
    match http.get_current_application_info().await {
        Ok(app) => Some(BotInstalls {
            servers: app.approximate_guild_count.map_or(0, u64::from),
            users: app.approximate_user_install_count.map_or(0, u64::from),
        }),
        Err(error) => {
            warn!(%error, "read the bot's install counts");
            None
        }
    }
}
