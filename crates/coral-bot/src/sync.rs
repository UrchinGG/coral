use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use serenity::all::*;

use database::{
    BlacklistRepository, CacheRepository, GuildConfig, GuildConfigRepository, GuildRoleRule,
    MemberRepository,
};

use crate::{expr, framework::Data};

pub const NICKNAME_MAX_LEN: usize = 32;
const REFRESH_THRESHOLD: Duration = Duration::from_secs(4 * 3600);


async fn yield_to_interactions(data: &Data) {
    while data.active_interactions.load(Ordering::Relaxed) > 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}


fn extract_custom_nickname(template: &str, template_ctx: &mut Value, current_nick: Option<&str>) -> String {
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
) -> Value {
    let mut ctx = hypixel_data.clone();

    ctx["discord"] = serde_json::json!({
        "name": member.user.global_name.as_deref().unwrap_or(&member.user.name),
    });

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
        .get_tags(uuid)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| row.tag_type)
        .collect()
}


pub async fn handle_member_update(ctx: &Context, data: &Data, member: &Member) {
    if member.user.bot() { return }
    let guild_id = member.guild_id;
    let discord_id = member.user.id.get() as i64;

    let config_repo = GuildConfigRepository::new(data.db.pool());
    let config = match config_repo.get(guild_id.get() as i64).await {
        Ok(Some(c)) => c,
        _ => return,
    };

    let rules = config_repo.get_role_rules(guild_id.get() as i64).await.unwrap_or_default();
    if config.nickname_template.is_none() && rules.is_empty() {
        return;
    }

    let uuid = match MemberRepository::new(data.db.pool())
        .get_by_discord_id(discord_id).await.ok().flatten().and_then(|m| m.uuid)
    {
        Some(uuid) => uuid,
        None => return,
    };

    let hypixel_data = match resolve_hypixel_data(data, &uuid).await {
        Some(d) => d,
        None => return,
    };

    if let Err(e) = sync_member(ctx, data, guild_id, member, &uuid, &config, &rules, &hypixel_data, true).await {
        tracing::debug!("Failed to sync member {} in {guild_id}: {e}", member.user.id);
    }
}


pub fn handle_message_activity(ctx: &Context, data: &Data, message: &Message) {
    if message.author.bot() { return }
    let Some(guild_id) = message.guild_id else { return };
    let user_id = message.author.id;
    if is_on_cooldown(data, user_id) { return }

    let ctx = ctx.clone();
    let data = data.clone();
    tokio::spawn(async move {
        if let Err(e) = try_sync_from_message(&ctx, &data, guild_id, user_id).await {
            tracing::warn!("Sync from message failed for {user_id} in {guild_id}: {e}");
        }
    });
}


pub async fn sync_user(ctx: Context, data: Data, user_id: UserId) {
    let uuid = match MemberRepository::new(data.db.pool())
        .get_by_discord_id(user_id.get() as i64).await.ok().flatten().and_then(|m| m.uuid)
    {
        Some(uuid) => uuid,
        None => return,
    };

    let hypixel_data = match resolve_hypixel_data(&data, &uuid).await {
        Some(hd) => hd,
        None => return,
    };

    let config_repo = GuildConfigRepository::new(data.db.pool());
    let configs = match config_repo.get_all().await {
        Ok(c) => c,
        Err(_) => return,
    };

    for config in configs {
        let guild_id = GuildId::new(config.guild_id as u64);
        let member = match guild_id.member(&ctx.http, user_id).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let rules = config_repo.get_role_rules(config.guild_id).await.unwrap_or_default();

        if let Err(e) = sync_member(&ctx, &data, guild_id, &member, &uuid, &config, &rules, &hypixel_data, false).await {
            tracing::warn!("User sync failed for {} in {guild_id}: {e}", user_id.get());
        }
    }
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
        .get_by_discord_id(user_id.get() as i64).await?.and_then(|m| m.uuid)
    {
        Some(uuid) => uuid,
        None => return Ok(()),
    };

    let hypixel_data = match resolve_hypixel_data(data, &uuid).await {
        Some(hd) => hd,
        None => return Ok(()),
    };

    let member = guild_id.member(&ctx.http, user_id).await?;
    sync_member(ctx, data, guild_id, &member, &uuid, &config, &rules, &hypixel_data, true).await?;
    Ok(())
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
    let template_ctx = build_template_context(hypixel_data, member, &tags);

    let mut roles: Vec<RoleId> = member.roles.iter().copied().collect();
    let original_roles = roles.clone();

    if let Some(id) = config.link_role_id {
        let role = RoleId::new(id as u64);
        if !roles.contains(&role) { roles.push(role) }
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

    let nickname = config.nickname_template.as_ref().and_then(|template| {
        let nick = if preserve_custom && template.contains("discord.nickname") {
            let mut ctx = template_ctx.clone();
            let custom = extract_custom_nickname(template, &mut ctx, member.nick.as_deref());
            ctx["discord"]["nickname"] = Value::String(custom);
            expr::render_template(template, &ctx).to_truncated(NICKNAME_MAX_LEN)
        } else {
            expr::render_template(template, &template_ctx).to_truncated(NICKNAME_MAX_LEN)
        };
        (!nick.is_empty() && member.nick.as_deref() != Some(&nick)).then_some(nick)
    });

    if !roles_changed && nickname.is_none() { return Ok(false) }

    yield_to_interactions(data).await;

    let mut edit = EditMember::new();
    if roles_changed { edit = edit.roles(&roles) }
    if let Some(ref nick) = nickname { edit = edit.nickname(nick) }
    guild_id.edit_member(&ctx.http, member.user.id, edit).await?;
    Ok(true)
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
    match cache.get_latest_non_migration_timestamp(uuid).await.ok().flatten() {
        Some(ts) => (Utc::now() - ts).num_seconds() > REFRESH_THRESHOLD.as_secs() as i64,
        None => true,
    }
}


fn is_on_cooldown(data: &Data, user_id: UserId) -> bool {
    data.sync_cooldowns.lock().unwrap()
        .get(&user_id)
        .is_some_and(|last| last.elapsed() < REFRESH_THRESHOLD)
}


fn set_cooldown(data: &Data, user_id: UserId) {
    let mut cooldowns = data.sync_cooldowns.lock().unwrap();
    cooldowns.retain(|_, last| last.elapsed() < REFRESH_THRESHOLD);
    cooldowns.insert(user_id, Instant::now());
}



