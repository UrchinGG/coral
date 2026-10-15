use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use database::{PluginRegistryRepository, StarfishRepository};
use serde::Serialize;
use sqlx::FromRow;

use super::parse_discord_id;
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;
use crate::serde_ts;
use crate::state::AppState;

#[derive(Serialize)]
pub struct StarfishAccount {
    user: StarfishUserView,
    hwids: Vec<HwidView>,
    sessions: Vec<SessionView>,
    refresh_tokens: Vec<RefreshTokenView>,
    installed_plugins: Vec<InstalledPlugin>,
    owned_plugins: Vec<OwnedPlugin>,
}

#[derive(Serialize)]
struct StarfishUserView {
    id: i64,
    license_status: String,
    github_username: Option<String>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    github_linked_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros")]
    created_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, FromRow)]
struct HwidView {
    id: i64,
    hwid_hash: String,
    is_active: bool,
    #[serde(serialize_with = "serde_ts::micros")]
    registered_at: DateTime<Utc>,
    machine_guid_hash: Option<String>,
    smbios_uuid_hash: Option<String>,
    disk_serial_hash: Option<String>,
    cpu_id_hash: Option<String>,
    baseboard_serial_hash: Option<String>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    components_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, FromRow)]
struct SessionView {
    id: i64,
    hwid_id: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    issued_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    expires_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    last_heartbeat_at: DateTime<Utc>,
    active: bool,
}

#[derive(Serialize, FromRow)]
struct RefreshTokenView {
    id: i64,
    hwid_id: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    created_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    last_used_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct InstalledPlugin {
    plugin: EntityRef,
    installed_version: String,
    latest_version: String,
    disabled: bool,
}

#[derive(Serialize)]
struct OwnedPlugin {
    plugin: EntityRef,
    official: bool,
    unlisted: bool,
    disabled: bool,
    latest_version: Option<String>,
}

pub async fn starfish(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<Option<StarfishAccount>>, AdminError> {
    let discord_id = parse_discord_id(&raw)?;
    let user = StarfishRepository::new(state.db.pool())
        .get_user_by_discord_id(discord_id)
        .await
        .context("read the account's Starfish user")?;
    let Some(user) = user else {
        return Ok(Json(None));
    };
    let (hwids, sessions, refresh_tokens, installed_plugins, owned_plugins) = tokio::try_join!(
        hwids(&state, user.id),
        sessions(&state, user.id),
        refresh_tokens(&state, user.id),
        installed_plugins(&state, user.id),
        owned_plugins(&state, user.id),
    )?;
    Ok(Json(Some(StarfishAccount {
        user: StarfishUserView {
            id: user.id,
            license_status: user.license_status,
            github_username: user.github_username,
            github_linked_at: user.github_linked_at,
            created_at: user.created_at,
            updated_at: user.updated_at,
        },
        hwids,
        sessions,
        refresh_tokens,
        installed_plugins,
        owned_plugins,
    })))
}

async fn hwids(state: &AppState, user_id: i64) -> Result<Vec<HwidView>, AdminError> {
    sqlx::query_as(
        "SELECT h.id, h.hwid_hash, h.is_active, h.registered_at,
                c.machine_guid_hash, c.smbios_uuid_hash, c.disk_serial_hash, c.cpu_id_hash,
                c.baseboard_serial_hash, c.created_at AS components_at
         FROM starfish_hwids h
         LEFT JOIN starfish_hwid_components c ON c.hwid_id = h.id
         WHERE h.user_id = $1
         ORDER BY h.is_active DESC, h.registered_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the Starfish HWIDs")
}

async fn sessions(state: &AppState, user_id: i64) -> Result<Vec<SessionView>, AdminError> {
    sqlx::query_as(
        "SELECT id, hwid_id, issued_at, expires_at, last_heartbeat_at, expires_at > now() AS active
         FROM starfish_sessions WHERE user_id = $1 ORDER BY issued_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the Starfish sessions")
}

async fn refresh_tokens(
    state: &AppState,
    user_id: i64,
) -> Result<Vec<RefreshTokenView>, AdminError> {
    sqlx::query_as(
        "SELECT id, hwid_id, created_at, last_used_at FROM starfish_refresh_tokens
         WHERE user_id = $1 ORDER BY last_used_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the Starfish refresh tokens")
}

async fn installed_plugins(
    state: &AppState,
    user_id: i64,
) -> Result<Vec<InstalledPlugin>, AdminError> {
    let installs = PluginRegistryRepository::new(state.db.pool())
        .list_user_installs(user_id)
        .await
        .context("read the installed plugins")?;
    Ok(installs
        .into_iter()
        .map(|install| InstalledPlugin {
            plugin: EntityRef::Plugin {
                slug: install.slug,
                name: None,
            },
            installed_version: install.installed_version,
            latest_version: install.latest_version,
            disabled: install.disabled,
        })
        .collect())
}

async fn owned_plugins(state: &AppState, user_id: i64) -> Result<Vec<OwnedPlugin>, AdminError> {
    let owned = PluginRegistryRepository::new(state.db.pool())
        .list_my_plugins_with_latest(user_id)
        .await
        .context("read the owned plugins")?;
    Ok(owned
        .into_iter()
        .map(|plugin| OwnedPlugin {
            plugin: EntityRef::Plugin {
                slug: plugin.slug,
                name: Some(plugin.display_name),
            },
            official: plugin.official,
            unlisted: plugin.unlisted,
            disabled: plugin.disabled,
            latest_version: plugin.latest_version,
        })
        .collect())
}
