use std::collections::{BTreeMap, HashMap};

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serenity::all::{ChannelId, ChannelType, GuildChannel, GuildId, Http, ImageHash, Role, RoleId};

use super::home_guild;
use crate::discord;
use crate::error::{AdminError, Context};
use crate::serde_ts;
use crate::state::AppState;
use crate::views::{CachedView, View};

const DIRECTORY_KEY: &str = "home";
const POPULATION_KEY: &str = "home";
const ICON_SIZE: u32 = 128;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshQuery {
    #[serde(default)]
    refresh: bool,
}

#[derive(Debug, Serialize)]
pub struct Directory {
    #[serde(serialize_with = "serde_ts::micros")]
    as_of: DateTime<Utc>,
    guild: GuildView,
    roles: Vec<RoleView>,
    channels: Vec<ChannelView>,
}

#[derive(Debug, Serialize)]
struct GuildView {
    id: String,
    name: String,
    icon_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleView {
    pub id: String,
    pub name: String,
    color: Option<String>,
    position: i64,
    managed: bool,
    assignable: bool,
    bot_assignable: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
struct ManagerTops {
    sync: i16,
    bot: Option<i16>,
}

#[derive(Debug, Serialize)]
struct ChannelView {
    id: String,
    name: String,
    category: Option<String>,
    kind: ChannelKind,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChannelKind {
    Text,
    Forum,
}

#[derive(Debug, Serialize)]
pub struct Population {
    #[serde(serialize_with = "serde_ts::micros")]
    as_of: DateTime<Utc>,
    member_count: usize,
    linked_members: i64,
    role_members: BTreeMap<String, usize>,
}

pub async fn directory(
    State(state): State<AppState>,
    query: Result<Query<RefreshQuery>, QueryRejection>,
) -> Result<impl IntoResponse, AdminError> {
    let Query(query) = query?;
    let guild_id = home_guild(&state)?;
    if query.refresh {
        state
            .views
            .forget(View::DiscordDirectory, DIRECTORY_KEY)
            .await;
    }
    state
        .views
        .get_or_load(View::DiscordDirectory, DIRECTORY_KEY, || {
            load_directory(&state, guild_id)
        })
        .await
}

pub async fn population(
    State(state): State<AppState>,
    query: Result<Query<RefreshQuery>, QueryRejection>,
) -> Result<CachedView, AdminError> {
    let Query(query) = query?;
    let guild_id = home_guild(&state)?;
    if query.refresh {
        state
            .views
            .forget(View::GuildPopulation, POPULATION_KEY)
            .await;
    }
    state
        .views
        .get_or_load(View::GuildPopulation, POPULATION_KEY, || {
            load_population(&state, guild_id)
        })
        .await
}

pub async fn guild_roles(http: &Http, guild_id: GuildId) -> Result<Vec<Role>, AdminError> {
    discord::guild_roles(http, guild_id)
        .await
        .map_err(|error| AdminError::Upstream(format!("Discord: read the server roles: {error:#}")))
}

async fn load_directory(state: &AppState, guild_id: GuildId) -> Result<Directory, AdminError> {
    let http = sync_http(state)?;
    let (guild, roles, channels, sync_roles, review_bot_roles) = tokio::try_join!(
        async {
            http.get_guild(guild_id)
                .await
                .map_err(|error| upstream("read the server", error))
        },
        guild_roles(http, guild_id),
        async {
            discord::guild_channels(http, guild_id)
                .await
                .map_err(|error| {
                    AdminError::Upstream(format!("Discord: read the channels: {error:#}"))
                })
        },
        own_roles(http, guild_id),
        review_bot_roles(state, guild_id),
    )?;
    let tops = ManagerTops {
        sync: top_position(&roles, &sync_roles),
        bot: review_bot_roles.map(|member_roles| top_position(&roles, &member_roles)),
    };
    Ok(Directory {
        as_of: Utc::now(),
        guild: GuildView {
            id: guild.id.to_string(),
            name: guild.name.to_string(),
            icon_url: icon_url(guild.id, guild.icon),
        },
        roles: role_views(&roles, tops, guild_id),
        channels: channel_views(&channels),
    })
}

async fn load_population(state: &AppState, guild_id: GuildId) -> Result<Population, AdminError> {
    let http = sync_http(state)?;
    let members = discord::list_guild_members(http, guild_id)
        .await
        .map_err(|error| {
            AdminError::Upstream(format!("Discord: list the server members: {error:#}"))
        })?;
    let humans: Vec<_> = members.iter().filter(|member| !member.user.bot()).collect();
    let ids: Vec<i64> = humans
        .iter()
        .map(|member| member.user.id.get() as i64)
        .collect();
    let linked_members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM members WHERE discord_id = ANY($1) AND uuid IS NOT NULL",
    )
    .bind(&ids)
    .fetch_one(state.db.pool())
    .await
    .context("count the linked server members")?;
    let mut role_members: BTreeMap<String, usize> = BTreeMap::new();
    for role in humans.iter().flat_map(|member| member.roles.iter()) {
        *role_members.entry(role.to_string()).or_default() += 1;
    }
    Ok(Population {
        as_of: Utc::now(),
        member_count: humans.len(),
        linked_members,
        role_members,
    })
}

async fn own_roles(http: &Http, guild_id: GuildId) -> Result<Vec<RoleId>, AdminError> {
    let user = http
        .get_current_user()
        .await
        .map_err(|error| upstream("read the bot user", error))?;
    let member = http
        .get_member(guild_id, user.id)
        .await
        .map_err(|error| upstream("read the bot's server roles", error))?;
    Ok(member.roles.into_iter().collect())
}

async fn review_bot_roles(
    state: &AppState,
    guild_id: GuildId,
) -> Result<Option<Vec<RoleId>>, AdminError> {
    let Ok(http) = state.discord.bot() else {
        return Ok(None);
    };
    own_roles(http, guild_id).await.map(Some)
}

fn sync_http(state: &AppState) -> Result<&Http, AdminError> {
    state
        .discord
        .sync()
        .map_err(|error| AdminError::Rejected(error.to_string()))
}

fn upstream(what: &str, error: serenity::Error) -> AdminError {
    AdminError::Upstream(format!("Discord: {what}: {error}"))
}

fn top_position(roles: &[Role], member_roles: &[RoleId]) -> i16 {
    roles
        .iter()
        .filter(|role| member_roles.contains(&role.id))
        .map(|role| role.position)
        .max()
        .unwrap_or(0)
}

fn role_views(roles: &[Role], tops: ManagerTops, guild_id: GuildId) -> Vec<RoleView> {
    let mut views: Vec<RoleView> = roles
        .iter()
        .filter(|role| role.id.get() != guild_id.get())
        .map(|role| RoleView {
            id: role.id.to_string(),
            name: role.name.to_string(),
            color: discord::role_color_hex(role.colour),
            position: i64::from(role.position),
            managed: role.managed(),
            assignable: manageable_below(role, tops.sync),
            bot_assignable: tops.bot.map(|top| manageable_below(role, top)),
        })
        .collect();
    views.sort_by(|a, b| b.position.cmp(&a.position));
    views
}

fn manageable_below(role: &Role, top: i16) -> bool {
    !role.managed() && role.position < top
}

fn channel_views(channels: &[GuildChannel]) -> Vec<ChannelView> {
    let categories: HashMap<ChannelId, &str> = channels
        .iter()
        .filter(|channel| channel.base.kind == ChannelType::Category)
        .map(|channel| (channel.id, channel.base.name.as_str()))
        .collect();
    channels
        .iter()
        .filter_map(|channel| {
            let kind = channel_kind(channel.base.kind)?;
            Some(ChannelView {
                id: channel.id.to_string(),
                name: channel.base.name.to_string(),
                category: channel
                    .parent_id
                    .and_then(|parent| categories.get(&parent).map(|name| name.to_string())),
                kind,
            })
        })
        .collect()
}

fn channel_kind(kind: ChannelType) -> Option<ChannelKind> {
    match kind {
        ChannelType::Text => Some(ChannelKind::Text),
        ChannelType::Forum => Some(ChannelKind::Forum),
        _ => None,
    }
}

fn icon_url(guild_id: GuildId, icon: Option<ImageHash>) -> Option<String> {
    icon.map(|hash| {
        format!("https://cdn.discordapp.com/icons/{guild_id}/{hash}.png?size={ICON_SIZE}")
    })
}
