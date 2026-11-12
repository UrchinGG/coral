use std::collections::HashMap as StdHashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;
use serenity::all::*;
use serenity::nonmax::NonMaxU16;
use tokio::sync::Semaphore;

use database::{GuildConfigRepository, MemberRepository, SyncSourceRepository};

use crate::engine::{self, Changes, Current, Settings};
use crate::framework::Data;
use crate::sources::{self, HYPIXEL_MAX_AGE, HypixelRead, Unavailable};
use database::SyncSource;

const BULK_CONCURRENCY: usize = 5;
const NICKNAME_BYPASS_ROLE_ENV: &str = "NICKNAME_BYPASS_ROLE_ID";
const MEMBERS_PER_PAGE: u16 = 1000;

pub type CancelToken = Arc<AtomicBool>;

/// A server's settings plus the external sources it chose.
/// Activity keeps a member's custom nickname part and leaves unlinked members alone;
/// a link change applies everything.
#[derive(Clone, Copy)]
enum Trigger {
    Link,
    Activity,
}

pub(crate) struct ServerSync {
    pub settings: Settings,
    pub sources: Vec<SyncSource>,
}

impl ServerSync {
    pub fn syncs_anything(&self) -> bool {
        self.settings.syncs_anything()
    }
}

pub fn cancel_token() -> CancelToken {
    Arc::new(AtomicBool::new(false))
}

#[derive(Clone, Default)]
pub struct Progress {
    inner: Option<Arc<ProgressInner>>,
}

struct ProgressInner {
    processed: std::sync::atomic::AtomicUsize,
    total: std::sync::atomic::AtomicUsize,
    report: Box<dyn Fn(usize, usize) + Send + Sync>,
}

impl Progress {
    pub fn none() -> Self {
        Self { inner: None }
    }

    pub fn new(report: impl Fn(usize, usize) + Send + Sync + 'static) -> Self {
        Self {
            inner: Some(Arc::new(ProgressInner {
                processed: std::sync::atomic::AtomicUsize::new(0),
                total: std::sync::atomic::AtomicUsize::new(0),
                report: Box::new(report),
            })),
        }
    }

    pub fn begin(&self, total: usize) {
        if let Some(inner) = &self.inner {
            inner.processed.store(0, Ordering::Relaxed);
            inner.total.store(total, Ordering::Relaxed);
            (inner.report)(0, total);
        }
    }

    pub fn incr(&self) {
        if let Some(inner) = &self.inner {
            let processed = inner.processed.fetch_add(1, Ordering::Relaxed) + 1;
            (inner.report)(processed, inner.total.load(Ordering::Relaxed));
        }
    }
}

async fn yield_to_interactions(data: &Data) {
    while data.active_interactions.load(Ordering::Relaxed) > 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn is_cancelled(token: &CancelToken) -> bool {
    token.load(Ordering::Relaxed)
}

pub async fn handle_member_update(ctx: &Context, data: &Data, member: &Member) {
    if member.user.bot() {
        return;
    }
    let guild_id = member.guild_id;
    let Ok(Some(settings)) = load_settings(data, guild_id).await else {
        return;
    };
    if !settings.syncs_anything() {
        return;
    }
    let Some(uuid) = linked_uuid(data, member.user.id).await else {
        return;
    };
    let Some(hypixel) = sources::hypixel(data, &uuid, HypixelRead::Fresh).await else {
        return;
    };

    if let Err(e) = sync_member(
        ctx, data, guild_id, member, &uuid, &settings, &hypixel, true,
    )
    .await
    {
        tracing::debug!(
            "Failed to sync member {} in {guild_id}: {e}",
            member.user.id
        );
    }
}

pub fn handle_message_activity(ctx: &Context, data: &Data, message: &Message) {
    if message.author.bot() {
        return;
    }
    if message.guild_id.is_none() {
        return;
    }
    let user_id = message.author.id;
    if is_on_cooldown(data, user_id) {
        return;
    }
    set_cooldown(data, user_id);

    let ctx = ctx.clone();
    let data = data.clone();
    tokio::spawn(async move { sync_everywhere(&ctx, &data, user_id, Trigger::Activity).await });
}

/// A link changed or a source reported a change: sync the member in every server.
pub async fn sync_user(ctx: Context, data: Data, user_id: UserId) {
    sync_everywhere(&ctx, &data, user_id, Trigger::Link).await;
}

async fn sync_everywhere(ctx: &Context, data: &Data, user_id: UserId, trigger: Trigger) {
    let configs = match GuildConfigRepository::new(data.db.pool()).get_all().await {
        Ok(configs) => configs,
        Err(e) => {
            tracing::warn!("Sync of {user_id} skipped: read the server configs: {e}");
            return;
        }
    };
    // shortcut: asks Discord whether the member is in each configured server; keep a membership index from member events once there are more than tens of servers.
    for config in configs {
        let guild_id = GuildId::new(config.guild_id as u64);
        if let Err(e) = sync_in(ctx, data, guild_id, user_id, trigger).await {
            tracing::warn!("Sync of {user_id} in {guild_id} failed: {e}");
        }
    }
}

async fn sync_in(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    user_id: UserId,
    trigger: Trigger,
) -> Result<()> {
    let Some(server) = load_settings(data, guild_id).await? else {
        return Ok(());
    };
    if matches!(trigger, Trigger::Activity) && !server.syncs_anything() {
        return Ok(());
    }
    let Ok(member) = guild_id.member(&ctx.http, user_id).await else {
        return Ok(());
    };
    let preserve_custom = matches!(trigger, Trigger::Activity);
    match linked_uuid(data, user_id).await {
        Some(uuid) => {
            let Some(hypixel) = sources::hypixel(data, &uuid, HypixelRead::Fresh).await else {
                return Ok(());
            };
            sync_member(
                ctx,
                data,
                guild_id,
                &member,
                &uuid,
                &server,
                &hypixel,
                preserve_custom,
            )
            .await?;
        }
        None if matches!(trigger, Trigger::Link) => {
            sync_unlinked_member(ctx, data, guild_id, &member, &server).await?;
        }
        None => {}
    }
    Ok(())
}

pub async fn sync_guild_cached(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    try_sync_guild(ctx, data, guild_id, cancel, progress).await
}

async fn try_sync_guild(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    let Some(settings) = load_settings(data, guild_id).await? else {
        return Ok(());
    };
    let settings = Arc::new(settings);

    let non_bots: Vec<_> = fetch_all_members(ctx, guild_id)
        .await?
        .into_iter()
        .filter(|m| !m.user.bot())
        .collect();
    let total = non_bots.len();
    progress.begin(total);

    let mut updates = 0usize;
    for chunk in non_bots.chunks(MEMBERS_PER_PAGE as usize) {
        if is_cancelled(cancel) {
            break;
        }
        let (_, page_updates) =
            sync_member_batch(ctx, data, guild_id, chunk, &settings, cancel, progress).await;
        updates += page_updates;
    }

    tracing::info!(
        "Guild sync {guild_id}: {updates}/{total} members updated{}",
        if is_cancelled(cancel) {
            " (cancelled)"
        } else {
            ""
        }
    );
    Ok(())
}

async fn sync_member_batch(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    members: &[Member],
    settings: &Arc<ServerSync>,
    cancel: &CancelToken,
    progress: &Progress,
) -> (usize, usize) {
    let members_repo = MemberRepository::new(data.db.pool());
    let discord_ids: Vec<i64> = members.iter().map(|m| m.user.id.get() as i64).collect();
    let uuid_map: StdHashMap<i64, String> = members_repo
        .get_linked_by_discord_ids(&discord_ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| m.uuid.map(|uuid| (m.discord_id, uuid)))
        .collect();

    let semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();

    for member in members {
        if member.user.bot() || is_cancelled(cancel) {
            continue;
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let ctx = ctx.clone();
        let data = data.clone();
        let member = member.clone();
        let settings = settings.clone();
        let uuid = uuid_map.get(&(member.user.id.get() as i64)).cloned();

        tasks.spawn(async move {
            let _permit = permit;

            let hypixel = match &uuid {
                Some(uuid) => sources::hypixel(&data, uuid, HypixelRead::Stored).await,
                None => None,
            };

            let result = match (uuid, hypixel) {
                (Some(uuid), Some(hypixel)) => {
                    sync_member(
                        &ctx, &data, guild_id, &member, &uuid, &settings, &hypixel, false,
                    )
                    .await
                }
                _ => sync_unlinked_member(&ctx, &data, guild_id, &member, &settings).await,
            };

            match result {
                Ok(changed) => changed,
                Err(e) => {
                    tracing::debug!("Sync failed for {} in {guild_id}: {e}", member.user.id);
                    false
                }
            }
        });
    }

    let mut total = 0;
    let mut updates = 0;
    while let Some(result) = tasks.join_next().await {
        total += 1;
        progress.incr();
        if matches!(result, Ok(true)) {
            updates += 1
        }
    }

    (total, updates)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn sync_member(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    member: &Member,
    uuid: &str,
    server: &ServerSync,
    hypixel: &Value,
    preserve_custom: bool,
) -> Result<bool> {
    let coral = sources::coral(data, member.user.id.get(), uuid).await;
    let discord_name = member
        .user
        .global_name
        .as_deref()
        .unwrap_or(&member.user.name);
    let mut context = sources::context(hypixel, Some(discord_name), &coral);
    for source in &server.sources {
        match sources::external(data, source, member.user.id.get()).await {
            Ok(Some(value)) => sources::add_external(&mut context, &source.name, value),
            Ok(None) => {}
            Err(Unavailable) => {
                tracing::debug!(
                    "Skipped {} in {guild_id}: {} has never answered for them",
                    member.user.id,
                    source.name
                );
                return Ok(false);
            }
        }
    }
    let roles: Vec<RoleId> = member.roles.iter().copied().collect();
    let current = Current {
        roles: &roles,
        nick: member.nick.as_deref(),
    };
    let nickname_frozen = coral.nickname_paused || holds_bypass_role(member);
    let changes = engine::linked_changes(
        &server.settings,
        &context,
        &current,
        nickname_frozen,
        preserve_custom,
    );
    apply(ctx, data, guild_id, member.user.id, changes).await
}

async fn sync_unlinked_member(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    member: &Member,
    server: &ServerSync,
) -> Result<bool> {
    let roles: Vec<RoleId> = member.roles.iter().copied().collect();
    let current = Current {
        roles: &roles,
        nick: member.nick.as_deref(),
    };
    let changes = engine::unlinked_changes(&server.settings, &current);
    apply(ctx, data, guild_id, member.user.id, changes).await
}

async fn apply(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    user_id: UserId,
    changes: Changes,
) -> Result<bool> {
    if changes.is_empty() {
        return Ok(false);
    }
    yield_to_interactions(data).await;

    let mut edit = EditMember::new();
    if let Some(roles) = &changes.roles {
        edit = edit.roles(roles)
    }
    if let Some(nick) = &changes.nickname {
        edit = edit.nickname(nick)
    }
    guild_id.edit_member(&ctx.http, user_id, edit).await?;
    Ok(true)
}

fn holds_bypass_role(member: &Member) -> bool {
    std::env::var(NICKNAME_BYPASS_ROLE_ENV)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|role| member.roles.contains(&RoleId::new(role)))
}

pub(crate) async fn load_settings(data: &Data, guild_id: GuildId) -> Result<Option<ServerSync>> {
    let repo = GuildConfigRepository::new(data.db.pool());
    let Some(config) = repo.get(guild_id.get() as i64).await? else {
        return Ok(None);
    };
    let rules = repo
        .get_role_rules(guild_id.get() as i64)
        .await
        .unwrap_or_default();
    let sources = SyncSourceRepository::new(data.db.pool())
        .for_guild(guild_id.get() as i64)
        .await?;
    Ok(Some(ServerSync {
        settings: Settings::new(&config, &rules),
        sources,
    }))
}

async fn linked_uuid(data: &Data, user_id: UserId) -> Option<String> {
    MemberRepository::new(data.db.pool())
        .get_by_discord_id(user_id.get() as i64)
        .await
        .ok()
        .flatten()
        .and_then(|m| m.uuid)
}

pub async fn clear_nicknames(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    let targets = scan_members(ctx, guild_id, |m| m.nick.is_some() && !m.user.bot()).await?;
    progress.begin(targets.len());

    for user_id in targets {
        if is_cancelled(cancel) {
            break;
        }
        let config = GuildConfigRepository::new(data.db.pool())
            .get(guild_id.get() as i64)
            .await?;
        if config
            .as_ref()
            .and_then(|c| c.nickname_template.as_ref())
            .is_some()
        {
            return Ok(());
        }
        yield_to_interactions(data).await;
        let _ = guild_id
            .edit_member(&ctx.http, user_id, EditMember::new().nickname(""))
            .await;
        progress.incr();
    }

    Ok(())
}

pub async fn clear_role(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    role_id: RoleId,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    let targets = scan_members(ctx, guild_id, |m| {
        !m.user.bot() && m.roles.contains(&role_id)
    })
    .await?;
    progress.begin(targets.len());

    for user_id in targets {
        if is_cancelled(cancel) {
            break;
        }
        yield_to_interactions(data).await;
        let _ = ctx
            .http
            .remove_member_role(guild_id, user_id, role_id, None)
            .await;
        progress.incr();
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub enum RoleConfigField {
    Link,
    Unlinked,
}

impl RoleConfigField {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "link" => Some(Self::Link),
            "unlinked" => Some(Self::Unlinked),
            _ => None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn swap_role(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    old_role: Option<RoleId>,
    new_role: Option<RoleId>,
    config_field: RoleConfigField,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    if old_role == new_role {
        return Ok(());
    }
    let Some(old) = old_role else { return Ok(()) };

    let targets = scan_members(ctx, guild_id, |m| !m.user.bot() && m.roles.contains(&old)).await?;
    progress.begin(targets.len());

    for user_id in targets {
        if is_cancelled(cancel) {
            break;
        }
        let current_config = GuildConfigRepository::new(data.db.pool())
            .get(guild_id.get() as i64)
            .await?;
        let current_role_id = current_config.as_ref().and_then(|c| match config_field {
            RoleConfigField::Link => c.link_role_id,
            RoleConfigField::Unlinked => c.unlinked_role_id,
        });
        if current_role_id != new_role.map(|r| r.get() as i64) {
            return Ok(());
        }
        yield_to_interactions(data).await;
        let _ = ctx
            .http
            .remove_member_role(guild_id, user_id, old, None)
            .await;
        progress.incr();
    }

    Ok(())
}

fn is_on_cooldown(data: &Data, user_id: UserId) -> bool {
    data.sync_cooldowns
        .lock()
        .unwrap()
        .get(&user_id)
        .is_some_and(|last| last.elapsed() < HYPIXEL_MAX_AGE)
}

fn set_cooldown(data: &Data, user_id: UserId) {
    let mut cooldowns = data.sync_cooldowns.lock().unwrap();
    cooldowns.retain(|_, last| last.elapsed() < HYPIXEL_MAX_AGE);
    cooldowns.insert(user_id, Instant::now());
}

pub async fn startup_sync(ctx: Context, data: Data) {
    let Some(guild_id) = data.home_guild_id else {
        tracing::info!("Startup sync skipped: HOME_GUILD_ID not configured");
        return;
    };

    match load_settings(&data, guild_id).await {
        Ok(Some(settings)) if settings.syncs_anything() => {}
        Ok(Some(_)) => return,
        Ok(None) => {
            tracing::info!("Startup sync skipped: home guild has no sync config");
            return;
        }
        Err(e) => {
            tracing::error!("Startup sync failed to load config: {e}");
            return;
        }
    }

    tracing::info!("Startup sync starting for guild {guild_id}");
    if let Err(e) = try_sync_guild(&ctx, &data, guild_id, &cancel_token(), &Progress::none()).await
    {
        tracing::warn!("Startup sync failed for guild {guild_id}: {e}");
    }
    tracing::info!("Startup sync complete");
}

async fn fetch_all_members(ctx: &Context, guild_id: GuildId) -> Result<Vec<Member>> {
    let mut all = Vec::new();
    let mut after = None;
    loop {
        let chunk = guild_id
            .members(
                &ctx.http,
                Some(NonMaxU16::new(MEMBERS_PER_PAGE).unwrap()),
                after,
            )
            .await?;
        if chunk.is_empty() {
            break;
        }
        after = chunk.last().map(|m| m.user.id);
        all.extend(chunk);
    }
    Ok(all)
}

async fn scan_members(
    ctx: &Context,
    guild_id: GuildId,
    predicate: impl Fn(&Member) -> bool,
) -> Result<Vec<UserId>> {
    let mut targets = Vec::new();
    let mut after = None;
    loop {
        let chunk = guild_id
            .members(
                &ctx.http,
                Some(NonMaxU16::new(MEMBERS_PER_PAGE).unwrap()),
                after,
            )
            .await?;
        if chunk.is_empty() {
            break;
        }
        after = chunk.last().map(|m| m.user.id);
        targets.extend(chunk.iter().filter(|m| predicate(m)).map(|m| m.user.id));
    }
    Ok(targets)
}
