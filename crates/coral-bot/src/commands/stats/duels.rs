use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use chrono::{DateTime, Utc};
use hypixel::parsing::duels_winstreaks;
use hypixel::{DuelsStats, DuelsView, DuelsWinstreakSnapshot, extract_duels_stats, extract_duels_winstreak_snapshot};
use image::DynamicImage;
use serenity::all::*;

use database::{CacheRepository, MemberRepository};
use render::TagIcon;

use crate::framework::Data;
use crate::rendering::render_duels;

use super::{
    CACHE_TTL_SECS, create_duels_dropdown, disable_components, encode_png, extract_select_value,
    extract_tag_icons, fetch_skin, parse_duels_value, resolve_uuid, send_deferred_error,
    spawn_expiry,
};


pub struct DuelsCache {
    pub stats: DuelsStats,
    pub skin: Option<DynamicImage>,
    pub tag_icons: Vec<TagIcon>,
    pub snapshots: Vec<(DateTime<Utc>, DuelsWinstreakSnapshot)>,
    pub view: DuelsView,
    pub sender_id: u64,
    pub last_interaction: Instant,
}


enum StatsError {
    PlayerNotFound,
    NoStats(String),
    ApiError,
}


enum CacheResult {
    Ok(Vec<u8>, CreateActionRow<'static>),
    Expired,
    Ephemeral(Vec<u8>),
}


pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("duels")
        .description("View a player's Duels stats")
        .add_option(CreateCommandOption::new(
            CommandOptionType::String,
            "player",
            "Player name or UUID",
        ))
}


pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let player_input = command
        .data
        .options
        .first()
        .and_then(|option| option.value.as_str())
        .map(|value| value.to_string());
    let sender_id = command.user.id.get();
    let cache_key = command.id.to_string();

    let player = match player_input {
        Some(player) => player,
        None => match MemberRepository::new(data.db.pool())
            .get_by_discord_id(sender_id as i64)
            .await
            .ok()
            .flatten()
            .and_then(|member| member.uuid)
        {
            Some(uuid) => uuid,
            None => {
                command.defer(&ctx.http).await?;
                return send_deferred_error(
                    ctx,
                    command,
                    "Not Linked",
                    "Link your account or provide a player name",
                )
                .await;
            }
        },
    };

    let (defer_result, result) =
        tokio::join!(command.defer(&ctx.http), fetch_player_data(data, &player));
    defer_result?;

    match result {
        Ok(mut cache) => {
            cache.sender_id = sender_id;
            let png = render_and_encode(&cache)?;
            let mode_row = CreateActionRow::SelectMenu(create_duels_dropdown(
                "duels_mode",
                &cache_key,
                cache.view,
                &cache.stats,
            ));
            let expiry_key = cache_key.clone();

            {
                let mut store = data.duels_images.lock().unwrap();
                evict_expired(&mut store);
                store.insert(cache_key, cache);
            }

            command
                .edit_response(
                    &ctx.http,
                    EditInteractionResponse::new()
                        .new_attachment(CreateAttachment::bytes(png, "duels.png"))
                        .components(vec![CreateComponent::ActionRow(mode_row)]),
                )
                .await?;

            spawn_expiry(
                ctx.http.clone(),
                command.token.to_string(),
                data.duels_images.clone(),
                expiry_key,
                |entry: &DuelsCache| entry.last_interaction,
            );
        }
        Err(StatsError::PlayerNotFound) => {
            send_deferred_error(
                ctx,
                command,
                "Player Not Found",
                &format!("Could not find player: {player}"),
            )
            .await?;
        }
        Err(StatsError::NoStats(username)) => {
            send_deferred_error(
                ctx,
                command,
                &format!("{username}'s Duels Stats"),
                "This player has no Duels stats",
            )
            .await?;
        }
        Err(StatsError::ApiError) => {
            send_deferred_error(
                ctx,
                command,
                "Error",
                "Something went wrong. Please try again later.",
            )
            .await?;
        }
    }

    Ok(())
}


pub async fn handle_mode_switch(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let Some(value) = extract_select_value(component) else {
        return Ok(());
    };
    let Some((cache_key, view)) = parse_duels_value(value) else {
        return Ok(());
    };

    match resolve_mode_switch(data, cache_key, view, component.user.id.get()) {
        CacheResult::Ok(png, mode_row) => {
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .add_file(CreateAttachment::bytes(png, "duels.png"))
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
                            .add_file(CreateAttachment::bytes(png, "duels.png"))
                            .ephemeral(true),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}


fn resolve_mode_switch(
    data: &Data,
    cache_key: &str,
    view: DuelsView,
    user_id: u64,
) -> CacheResult {
    let mut store = data.duels_images.lock().unwrap();

    let Some(entry) = store.get_mut(cache_key) else {
        return CacheResult::Expired;
    };
    if entry.last_interaction.elapsed().as_secs() > CACHE_TTL_SECS {
        store.remove(cache_key);
        return CacheResult::Expired;
    }
    if entry.sender_id != user_id {
        return render_ephemeral(entry, view);
    }

    entry.view = view;
    entry.last_interaction = Instant::now();
    let mode_row = CreateActionRow::SelectMenu(create_duels_dropdown(
        "duels_mode",
        cache_key,
        view,
        &entry.stats,
    ));

    match render_and_encode(entry) {
        Ok(png) => CacheResult::Ok(png, mode_row),
        Err(_) => CacheResult::Expired,
    }
}


fn render_ephemeral(entry: &mut DuelsCache, view: DuelsView) -> CacheResult {
    let original = entry.view;
    entry.view = view;
    let result = render_and_encode(entry);
    entry.view = original;
    match result {
        Ok(png) => CacheResult::Ephemeral(png),
        Err(_) => CacheResult::Expired,
    }
}


fn render_and_encode(cache: &DuelsCache) -> Result<Vec<u8>> {
    let winstreaks = duels_winstreaks::calculate(&cache.snapshots, cache.view);
    encode_png(&render_duels(
        &cache.stats,
        cache.view,
        cache.skin.as_ref(),
        &winstreaks,
        &cache.tag_icons,
    ))
}


fn evict_expired(cache: &mut HashMap<String, DuelsCache>) {
    cache.retain(|_, entry| entry.last_interaction.elapsed().as_secs() <= CACHE_TTL_SECS);
}


async fn fetch_player_data(data: &Data, player: &str) -> Result<DuelsCache, StatsError> {
    let cached_uuid = resolve_uuid(data, player).await;

    let (resp, guild_result, skin_result, snapshots) = match cached_uuid {
        Some(ref uuid) => {
            let cache_repo = CacheRepository::new(data.db.pool());
            let (api, guild, skin, history) = tokio::join!(
                data.api.get_player_stats(player),
                data.api.get_guild(uuid, Some("player")),
                data.skin_provider.fetch(uuid),
                cache_repo.get_all_snapshots_mapped(uuid, extract_duels_winstreak_snapshot),
            );
            let resp = api.map_err(map_api_error)?;
            if resp.uuid == *uuid {
                (resp, guild, skin, history)
            } else {
                let cache_repo = CacheRepository::new(data.db.pool());
                let (guild, skin, history) = tokio::join!(
                    data.api.get_guild(&resp.uuid, Some("player")),
                    fetch_skin(data, &resp.uuid, resp.skin_url.as_deref(), resp.slim),
                    cache_repo.get_all_snapshots_mapped(&resp.uuid, extract_duels_winstreak_snapshot),
                );
                (resp, guild, skin, history)
            }
        }
        None => {
            let resp = data
                .api
                .get_player_stats(player)
                .await
                .map_err(map_api_error)?;
            let cache_repo = CacheRepository::new(data.db.pool());
            let (guild, skin, history) = tokio::join!(
                data.api.get_guild(&resp.uuid, Some("player")),
                fetch_skin(data, &resp.uuid, resp.skin_url.as_deref(), resp.slim),
                cache_repo.get_all_snapshots_mapped(&resp.uuid, extract_duels_winstreak_snapshot),
            );
            (resp, guild, skin, history)
        }
    };

    let hypixel_data = resp.hypixel.ok_or(StatsError::PlayerNotFound)?;
    let username = resp.username.clone();
    let guild_info = guild_result
        .ok()
        .flatten()
        .map(|guild| super::to_guild_info(&guild));
    let stats = extract_duels_stats(&username, &hypixel_data, guild_info)
        .ok_or_else(|| StatsError::NoStats(username.clone()))?;

    Ok(DuelsCache {
        stats,
        skin: skin_result.map(|value| value.data),
        tag_icons: extract_tag_icons(&resp.tags),
        snapshots: snapshots.unwrap_or_default(),
        view: DuelsView::Overall,
        sender_id: 0,
        last_interaction: Instant::now(),
    })
}


fn map_api_error(error: crate::api::ApiError) -> StatsError {
    match error {
        crate::api::ApiError::NotFound => StatsError::PlayerNotFound,
        other => {
            tracing::error!("Internal API error: {other}");
            StatsError::ApiError
        }
    }
}
