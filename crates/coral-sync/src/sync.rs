use std::collections::HashMap as StdHashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use serenity::all::*;
use serenity::nonmax::NonMaxU16;
use tokio::sync::Semaphore;

use database::{
    BlacklistRepository, CacheRepository, GuildConfig, GuildConfigRepository, GuildRoleRule,
    MemberRepository,
};

use crate::framework::Data;

pub const NICKNAME_MAX_LEN: usize = 32;
const BULK_CONCURRENCY: usize = 5;
const REFRESH_THRESHOLD: Duration = Duration::from_secs(4 * 3600);
const MEMBERS_PER_PAGE: u16 = 1000;

pub type CancelToken = Arc<AtomicBool>;

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

pub(crate) fn is_home_guild(data: &Data, guild_id: GuildId) -> bool {
    data.home_guild_id == Some(guild_id)
}

fn extract_custom_nickname(
    template: &str,
    template_ctx: &mut Value,
    current_nick: Option<&str>,
) -> String {
    let current = match current_nick {
        Some(nick) if !nick.is_empty() => nick,
        _ => return String::new(),
    };

    template_ctx["discord"]["nickname"] = Value::String(String::new());
    let base = expr::render_template(template, template_ctx).to_truncated(NICKNAME_MAX_LEN);

    current
        .strip_prefix(base.trim_end())
        .map(|rest| rest.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

pub(crate) fn build_template_context(
    hypixel_data: &Value,
    member: &Member,
    active_tags: &[String],
    coral_access: i16,
    tag_granted: bool,
) -> Value {
    let mut ctx = hypixel_data.clone();

    if ctx.pointer("/achievements/bedwars_level").is_none() {
        ctx["achievements"]["bedwars_level"] = serde_json::json!(0);
    }
    let active_bracket = ctx
        .pointer("/stats/Bedwars/active_prestige_bracket")
        .and_then(|v| v.as_str());
    let (bracket_open, bracket_close) = hypixel::nickname_bracket_glyphs(active_bracket);
    ctx["bedwars"] = serde_json::json!({
        "bracket_open": bracket_open,
        "bracket_close": bracket_close,
    });

    ctx["discord"] = serde_json::json!({
        "name": member.user.global_name.as_deref().unwrap_or(&member.user.name),
    });
    ctx["coral"] = serde_json::json!({ "access": coral_access });
    ctx["standing"] = serde_json::json!({ "trusted": tag_granted });

    let highest = active_tags
        .iter()
        .filter_map(|t| blacklist::lookup(t).map(|def| (def.priority, t.as_str())))
        .min_by_key(|(p, _)| *p)
        .map(|(_, name)| name);

    let mut bl = serde_json::json!({ "tag": highest });
    for def in blacklist::all() {
        bl[def.name] = Value::Bool(active_tags.iter().any(|t| t == def.name));
    }
    ctx["blacklist"] = bl;
    ctx
}

pub(crate) async fn active_tags(data: &Data, uuid: &str) -> Vec<String> {
    BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| row.tag_type)
        .collect()
}

pub async fn handle_member_update(ctx: &Context, data: &Data, member: &Member) {
    if member.user.bot() {
        return;
    }
    let guild_id = member.guild_id;
    if !is_home_guild(data, guild_id) {
        return;
    }
    let discord_id = member.user.id.get() as i64;

    let config_repo = GuildConfigRepository::new(data.db.pool());
    let config = match config_repo.get(guild_id.get() as i64).await {
        Ok(Some(c)) => c,
        _ => return,
    };

    let rules = config_repo
        .get_role_rules(guild_id.get() as i64)
        .await
        .unwrap_or_default();
    if config.nickname_template.is_none() && rules.is_empty() {
        return;
    }

    let uuid = match MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id)
        .await
        .ok()
        .flatten()
        .and_then(|m| m.uuid)
    {
        Some(uuid) => uuid,
        None => return,
    };

    let hypixel_data = match resolve_hypixel_data(data, &uuid).await {
        Some(d) => d,
        None => return,
    };

    if let Err(e) = sync_member(
        ctx,
        data,
        guild_id,
        member,
        &uuid,
        &config,
        &rules,
        &hypixel_data,
        true,
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
    let Some(guild_id) = message.guild_id else {
        return;
    };
    if !is_home_guild(data, guild_id) {
        return;
    }
    let user_id = message.author.id;
    if is_on_cooldown(data, user_id) {
        return;
    }

    let ctx = ctx.clone();
    let data = data.clone();
    tokio::spawn(async move {
        if let Err(e) = try_sync_from_message(&ctx, &data, guild_id, user_id).await {
            tracing::warn!("Sync from message failed for {user_id} in {guild_id}: {e}");
        }
    });
}

pub async fn sync_user(ctx: Context, data: Data, user_id: UserId) {
    let Some(guild_id) = data.home_guild_id else {
        tracing::debug!("User sync skipped: HOME_GUILD_ID not configured");
        return;
    };

    let config_repo = GuildConfigRepository::new(data.db.pool());
    let config = match config_repo.get(guild_id.get() as i64).await {
        Ok(Some(c)) => c,
        _ => return,
    };
    let rules = config_repo
        .get_role_rules(guild_id.get() as i64)
        .await
        .unwrap_or_default();

    let member = match guild_id.member(&ctx.http, user_id).await {
        Ok(m) => m,
        Err(_) => return,
    };

    let uuid = MemberRepository::new(data.db.pool())
        .get_by_discord_id(user_id.get() as i64)
        .await
        .ok()
        .flatten()
        .and_then(|m| m.uuid);

    let result = match uuid {
        Some(uuid) => {
            let Some(hypixel_data) = resolve_hypixel_data(&data, &uuid).await else {
                return;
            };
            sync_member(
                &ctx,
                &data,
                guild_id,
                &member,
                &uuid,
                &config,
                &rules,
                &hypixel_data,
                false,
            )
            .await
        }
        None => sync_unlinked_member(&ctx, &data, guild_id, &member, &config, &rules).await,
    };

    if let Err(e) = result {
        tracing::warn!("User sync failed for {} in {guild_id}: {e}", user_id.get());
    }
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

async fn try_sync_from_message(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<()> {
    set_cooldown(data, user_id);

    let config_repo = GuildConfigRepository::new(data.db.pool());
    let config = match config_repo.get(guild_id.get() as i64).await? {
        Some(config) => config,
        None => return Ok(()),
    };
    let rules = config_repo.get_role_rules(guild_id.get() as i64).await?;
    if config.nickname_template.is_none() && rules.is_empty() {
        return Ok(());
    }

    let uuid = match MemberRepository::new(data.db.pool())
        .get_by_discord_id(user_id.get() as i64)
        .await?
        .and_then(|m| m.uuid)
    {
        Some(uuid) => uuid,
        None => return Ok(()),
    };

    let hypixel_data = match resolve_hypixel_data(data, &uuid).await {
        Some(hd) => hd,
        None => return Ok(()),
    };

    let member = guild_id.member(&ctx.http, user_id).await?;
    sync_member(
        ctx,
        data,
        guild_id,
        &member,
        &uuid,
        &config,
        &rules,
        &hypixel_data,
        true,
    )
    .await?;
    Ok(())
}

async fn try_sync_guild(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    cancel: &CancelToken,
    progress: &Progress,
) -> Result<()> {
    let config_repo = GuildConfigRepository::new(data.db.pool());
    let config = match config_repo.get(guild_id.get() as i64).await? {
        Some(config) => config,
        None => return Ok(()),
    };
    let rules = config_repo.get_role_rules(guild_id.get() as i64).await?;

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
        let (_, page_updates) = sync_member_batch(
            ctx, data, guild_id, chunk, &config, &rules, cancel, progress,
        )
        .await;
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

#[allow(clippy::too_many_arguments)]
async fn sync_member_batch(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    members: &[Member],
    config: &GuildConfig,
    rules: &[GuildRoleRule],
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
        let config = config.clone();
        let rules = rules.to_vec();
        let uuid = uuid_map.get(&(member.user.id.get() as i64)).cloned();

        tasks.spawn(async move {
            let _permit = permit;

            let snapshot = match &uuid {
                Some(uuid) => CacheRepository::new(data.db.pool())
                    .get_latest_snapshot(uuid)
                    .await
                    .ok()
                    .flatten(),
                None => None,
            };

            let result = match (uuid, snapshot) {
                (Some(uuid), Some(hd)) => {
                    sync_member(
                        &ctx, &data, guild_id, &member, &uuid, &config, &rules, &hd, false,
                    )
                    .await
                }
                _ => sync_unlinked_member(&ctx, &data, guild_id, &member, &config, &rules).await,
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

pub(crate) async fn sync_member(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    member: &Member,
    uuid: &str,
    config: &GuildConfig,
    rules: &[GuildRoleRule],
    hypixel_data: &Value,
    preserve_custom: bool,
) -> Result<bool> {
    let tags = active_tags(data, uuid).await;
    let member_db = MemberRepository::new(data.db.pool())
        .get_by_discord_id(member.user.id.get() as i64)
        .await
        .ok()
        .flatten();
    let access = member_db.as_ref().map(|m| m.access_level).unwrap_or(0);
    let tag_granted = member_db.as_ref().is_some_and(|m| m.tag_granted);
    let template_ctx = build_template_context(hypixel_data, member, &tags, access, tag_granted);

    let mut roles: Vec<RoleId> = member.roles.iter().copied().collect();
    let original_roles = roles.clone();

    if let Some(id) = config.link_role_id {
        let role = RoleId::new(id as u64);
        if !roles.contains(&role) {
            roles.push(role)
        }
    }
    if let Some(id) = config.unlinked_role_id {
        roles.retain(|r| *r != RoleId::new(id as u64));
    }

    for rule in rules {
        let role = RoleId::new(rule.role_id as u64);
        let matches = expr::eval_condition(&rule.condition, &template_ctx).unwrap_or(false);
        if matches && !roles.contains(&role) {
            roles.push(role);
        } else if !matches {
            roles.retain(|r| *r != role);
        }
    }

    let roles_changed = roles != original_roles;

    let pause_name_updates = member_db
        .as_ref()
        .and_then(|m| m.config.get("pause_name_updates"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let bypass_role = std::env::var("NICKNAME_BYPASS_ROLE_ID")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(RoleId::new);
    let nickname_bypassed = bypass_role.is_some_and(|r| member.roles.contains(&r));

    let nickname = if nickname_bypassed || pause_name_updates {
        None
    } else {
        config.nickname_template.as_ref().and_then(|template| {
            let nick = if preserve_custom && template.contains("discord.nickname") {
                let mut ctx = template_ctx.clone();
                let custom = extract_custom_nickname(template, &mut ctx, member.nick.as_deref());
                ctx["discord"]["nickname"] = Value::String(custom);
                expr::render_template(template, &ctx).to_truncated(NICKNAME_MAX_LEN)
            } else {
                expr::render_template(template, &template_ctx).to_truncated(NICKNAME_MAX_LEN)
            };
            (!nick.is_empty() && member.nick.as_deref() != Some(&nick)).then_some(nick)
        })
    };

    if !roles_changed && nickname.is_none() {
        return Ok(false);
    }

    yield_to_interactions(data).await;

    let mut edit = EditMember::new();
    if roles_changed {
        edit = edit.roles(&roles)
    }
    if let Some(ref nick) = nickname {
        edit = edit.nickname(nick)
    }
    guild_id
        .edit_member(&ctx.http, member.user.id, edit)
        .await?;
    Ok(true)
}

async fn sync_unlinked_member(
    ctx: &Context,
    data: &Data,
    guild_id: GuildId,
    member: &Member,
    config: &GuildConfig,
    rules: &[GuildRoleRule],
) -> Result<bool> {
    let mut roles: Vec<RoleId> = member.roles.iter().copied().collect();
    let original_roles = roles.clone();

    if let Some(id) = config.unlinked_role_id {
        let role = RoleId::new(id as u64);
        if !roles.contains(&role) {
            roles.push(role)
        }
    }
    if let Some(id) = config.link_role_id {
        roles.retain(|r| *r != RoleId::new(id as u64));
    }
    for rule in rules {
        roles.retain(|r| *r != RoleId::new(rule.role_id as u64));
    }

    if roles == original_roles {
        return Ok(false);
    }

    yield_to_interactions(data).await;
    guild_id
        .edit_member(&ctx.http, member.user.id, EditMember::new().roles(&roles))
        .await?;
    Ok(true)
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

async fn resolve_hypixel_data(data: &Data, uuid: &str) -> Option<Value> {
    let cache = CacheRepository::new(data.db.pool());

    if is_snapshot_stale(&cache, uuid).await {
        match data.api.get_player_stats(uuid).await {
            Ok(response) => return response.hypixel,
            Err(e) => tracing::debug!("Hypixel refresh failed for {uuid}, using cache: {e}"),
        }
    }

    cache.get_latest_snapshot(uuid).await.ok().flatten()
}

async fn is_snapshot_stale(cache: &CacheRepository<'_>, uuid: &str) -> bool {
    match cache
        .get_latest_non_migration_timestamp(uuid)
        .await
        .ok()
        .flatten()
    {
        Some(ts) => (Utc::now() - ts).num_seconds() > REFRESH_THRESHOLD.as_secs() as i64,
        None => true,
    }
}

fn is_on_cooldown(data: &Data, user_id: UserId) -> bool {
    data.sync_cooldowns
        .lock()
        .unwrap()
        .get(&user_id)
        .is_some_and(|last| last.elapsed() < REFRESH_THRESHOLD)
}

fn set_cooldown(data: &Data, user_id: UserId) {
    let mut cooldowns = data.sync_cooldowns.lock().unwrap();
    cooldowns.retain(|_, last| last.elapsed() < REFRESH_THRESHOLD);
    cooldowns.insert(user_id, Instant::now());
}

pub async fn startup_sync(ctx: Context, data: Data) {
    let Some(guild_id) = data.home_guild_id else {
        tracing::info!("Startup sync skipped: HOME_GUILD_ID not configured");
        return;
    };

    let config_repo = GuildConfigRepository::new(data.db.pool());
    let config = match config_repo.get(guild_id.get() as i64).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            tracing::info!("Startup sync skipped: home guild has no sync config");
            return;
        }
        Err(e) => {
            tracing::error!("Startup sync failed to load config: {e}");
            return;
        }
    };
    let rules = config_repo
        .get_role_rules(guild_id.get() as i64)
        .await
        .unwrap_or_default();
    if config.nickname_template.is_none() && rules.is_empty() {
        return;
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
