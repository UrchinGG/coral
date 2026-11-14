use axum::extract::State;
use database::{
    BlacklistRepository, CacheRepository, GuildCacheRepository, GuildCurrentRepository,
    MemberRepository,
};
use serde::Serialize;
use tracing::warn;

use crate::error::{AdminError, Context};
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
}

#[derive(Serialize)]
struct BotInstalls {
    servers: u64,
    users: u64,
}

#[derive(Serialize)]
struct Members {
    registered: i64,
    trusted: i64,
    lifetime_requests: i64,
}

#[derive(Serialize)]
struct Blacklist {
    active: i64,
    by_type: Vec<TagCount>,
}

#[derive(Serialize)]
struct TagCount {
    tag_type: String,
    count: i64,
}

#[derive(Serialize)]
struct Cached {
    tracked: i64,
    snapshots: i64,
    storage_bytes: i64,
}

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
    let (counts, bot) = tokio::join!(
        async {
            tokio::try_join!(
                members.count(),
                members.count_trusted(),
                members.total_requests(),
                blacklist.count_active_tags(),
                blacklist.count_active_tags_by_type(),
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
    let (
        registered,
        trusted,
        requests,
        active,
        by_type,
        players,
        snapshots,
        storage,
        guilds,
        guild_snaps,
        guild_storage,
    ) = counts.context("count the overview")?;
    Ok(Overview {
        bot,
        members: Members {
            registered,
            trusted,
            lifetime_requests: requests,
        },
        blacklist: Blacklist {
            active,
            by_type: by_type
                .into_iter()
                .map(|(tag_type, count)| TagCount { tag_type, count })
                .collect(),
        },
        players: Cached {
            tracked: players,
            snapshots,
            storage_bytes: storage,
        },
        guilds: Cached {
            tracked: guilds,
            snapshots: guild_snaps,
            storage_bytes: guild_storage,
        },
    })
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
