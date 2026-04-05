use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use serenity::all::*;
use tracing::debug;

use database::{CacheRepository, MemberRepository};

use crate::framework::Data;
use super::{
    CACHE_TTL_SECS, GameStats, OverallCache, StatsError, disable_components, evict_expired,
    extract_tag_icons, fetch_skin, map_api_error, resolve_uuid, send_deferred_error, spawn_expiry,
};


enum CacheResult<'a> {
    Ok(Vec<u8>, CreateActionRow<'a>),
    Expired,
    Ephemeral(Vec<u8>),
}


pub(super) async fn run<G: GameStats>(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let t = Instant::now();

    let player_input = command.data.options.first()
        .and_then(|o| o.value.as_str())
        .map(|s| s.to_string());
    let sender_id = command.user.id.get();
    let cache_key = command.id.to_string();

    let player = match player_input {
        Some(p) => p,
        None => {
            match MemberRepository::new(data.db.pool())
                .get_by_discord_id(sender_id as i64)
                .await
                .ok()
                .flatten()
                .and_then(|m| m.uuid)
            {
                Some(uuid) => uuid,
                None => {
                    command.defer(&ctx.http).await?;
                    return send_deferred_error(ctx, command, "Not Linked", "Link your account or provide a player name").await;
                }
            }
        }
    };

    let (defer_result, result) = tokio::join!(
        command.defer(&ctx.http),
        fetch_player_data::<G>(data, &player),
    );
    defer_result?;
    debug!(at = ?t.elapsed(), "fetch done");

    match result {
        Ok(mut cache) => {
            cache.sender_id = sender_id;
            let png = render_and_encode::<G>(&cache)?;
            debug!(at = ?t.elapsed(), "render done");

            let mode_row = CreateActionRow::SelectMenu(G::create_mode_dropdown(
                G::OVERALL_MODE_ID, &cache_key, &cache.mode, &cache.stats,
            ));
            let expiry_key = cache_key.clone();

            {
                let mut store = G::overall_cache(data).lock().unwrap();
                evict(&mut store);
                store.insert(cache_key, cache);
            }

            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .new_attachment(CreateAttachment::bytes(png, G::ATTACHMENT_NAME))
                        .components(vec![CreateComponent::ActionRow(mode_row)]),
                )
                .await?;

            spawn_expiry(
                ctx.http.clone(),
                command.token.to_string(),
                G::overall_cache(data).clone(),
                expiry_key,
                |e: &OverallCache<G>| e.last_interaction,
            );
            debug!(player = %player, at = ?t.elapsed(), "send done");
        }
        Err(StatsError::PlayerNotFound) => {
            send_deferred_error(ctx, command, "Player Not Found", &format!("Could not find player: {player}")).await?;
        }
        Err(StatsError::NoStats(username)) => {
            send_deferred_error(ctx, command, &format!("{username}'s {game} Stats", game = G::GAME_NAME), &format!("This player has no {} stats", G::GAME_NAME)).await?;
        }
        Err(StatsError::ApiError) => {
            send_deferred_error(ctx, command, "Error", "Something went wrong. Please try again later.").await?;
        }
    }

    Ok(())
}


pub(super) async fn handle_mode_switch<G: GameStats>(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let Some((cache_key, mode)) = G::parse_mode_interaction(component) else { return Ok(()) };

    match resolve_mode_switch::<G>(data, &cache_key, mode.clone(), component.user.id.get()) {
        CacheResult::Ok(png, mode_row) => {
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .add_file(CreateAttachment::bytes(png, G::ATTACHMENT_NAME))
                            .components(vec![CreateComponent::ActionRow(mode_row)]),
                    ),
                )
                .await?;
        }
        CacheResult::Expired => {
            disable_components(ctx, component).await?;
        }
        CacheResult::Ephemeral(png) => {
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .add_file(CreateAttachment::bytes(png, G::ATTACHMENT_NAME))
                            .ephemeral(true),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}


fn resolve_mode_switch<G: GameStats>(data: &Data, cache_key: &str, mode: G::ModeSelection, user_id: u64) -> CacheResult<'static> {
    let mut store = G::overall_cache(data).lock().unwrap();

    let Some(entry) = store.get_mut(cache_key) else {
        return CacheResult::Expired;
    };
    if entry.last_interaction.elapsed().as_secs() > CACHE_TTL_SECS {
        store.remove(cache_key);
        return CacheResult::Expired;
    }
    if entry.sender_id != user_id {
        return render_ephemeral::<G>(entry, &mode);
    }

    entry.mode = mode;
    entry.last_interaction = Instant::now();
    let mode_row = CreateActionRow::SelectMenu(G::create_mode_dropdown(
        G::OVERALL_MODE_ID, cache_key, &entry.mode, &entry.stats,
    ));

    match render_and_encode::<G>(entry) {
        Ok(png) => CacheResult::Ok(png, mode_row),
        Err(_) => CacheResult::Expired,
    }
}


fn render_ephemeral<G: GameStats>(entry: &mut OverallCache<G>, mode: &G::ModeSelection) -> CacheResult<'static> {
    let original = std::mem::replace(&mut entry.mode, mode.clone());
    let result = render_and_encode::<G>(entry);
    entry.mode = original;
    match result {
        Ok(png) => CacheResult::Ephemeral(png),
        Err(_) => CacheResult::Expired,
    }
}


fn render_and_encode<G: GameStats>(cache: &OverallCache<G>) -> Result<Vec<u8>> {
    G::render_overall(&cache.stats, &cache.mode, cache.skin.as_ref(), &cache.snapshots, &cache.tag_icons)
}


async fn fetch_player_data<G: GameStats>(data: &Data, player: &str) -> Result<OverallCache<G>, StatsError> {
    let t = Instant::now();
    let cached_uuid = resolve_uuid(data, player).await;
    debug!(at = ?t.elapsed(), cached = cached_uuid.is_some(), "resolve");

    let (resp, guild_result, skin_result, history_result) = match cached_uuid {
        Some(ref uuid) => {
            let cache_repo = CacheRepository::new(data.db.pool());
            let (api, guild, skin, history) = tokio::join!(
                data.api.get_player_stats(player),
                data.api.get_guild(uuid, Some("player")),
                data.skin_provider.fetch(uuid),
                cache_repo.get_all_snapshots_mapped(uuid, G::extract_winstreak_snapshot),
            );
            let resp = api.map_err(map_api_error)?;

            if resp.uuid == *uuid {
                (resp, guild, skin, history)
            } else {
                let cache_repo = CacheRepository::new(data.db.pool());
                let (guild, skin, history) = tokio::join!(
                    data.api.get_guild(&resp.uuid, Some("player")),
                    fetch_skin(data, &resp.uuid, resp.skin_url.as_deref(), resp.slim),
                    cache_repo.get_all_snapshots_mapped(&resp.uuid, G::extract_winstreak_snapshot),
                );
                (resp, guild, skin, history)
            }
        }
        None => {
            let resp = data.api.get_player_stats(player).await.map_err(map_api_error)?;
            let cache_repo = CacheRepository::new(data.db.pool());
            let (guild, skin, history) = tokio::join!(
                data.api.get_guild(&resp.uuid, Some("player")),
                fetch_skin(data, &resp.uuid, resp.skin_url.as_deref(), resp.slim),
                cache_repo.get_all_snapshots_mapped(&resp.uuid, G::extract_winstreak_snapshot),
            );
            (resp, guild, skin, history)
        }
    };
    debug!(at = ?t.elapsed(), "api done");

    let hypixel_data = resp.hypixel.ok_or(StatsError::PlayerNotFound)?;
    let username = resp.username.clone();
    let guild_info = guild_result.ok().flatten().map(|g| super::to_guild_info(&g));
    let stats = G::extract_stats(&username, &hypixel_data, guild_info)
        .ok_or_else(|| StatsError::NoStats(username.clone()))?;
    let snapshots = history_result.ok().unwrap_or_default();
    debug!(at = ?t.elapsed(), snapshots = snapshots.len(), "parse done");

    let mode = G::default_mode(&stats);

    Ok(OverallCache {
        stats,
        skin: skin_result.map(|s| s.data),
        tag_icons: extract_tag_icons(&resp.tags),
        snapshots,
        mode,
        sender_id: 0,
        last_interaction: Instant::now(),
    })
}


fn evict<G: GameStats>(cache: &mut HashMap<String, OverallCache<G>>) {
    evict_expired(cache, |e| e.last_interaction);
}
