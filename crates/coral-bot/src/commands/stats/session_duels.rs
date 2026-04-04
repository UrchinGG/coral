use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use hypixel::parsing::duels_winstreaks;
use hypixel::{DuelsStats, DuelsView, DuelsWinstreakSnapshot, extract_duels_stats, extract_duels_winstreak_snapshot};
use image::DynamicImage;
use serenity::all::*;

use database::{
    AccountRepository, CacheRepository, MemberRepository, Period, SessionMarker, SessionRepository,
};
use render::TagIcon;

use crate::framework::Data;
use crate::rendering::{SessionType, render_duels_session};
use super::{
    CACHE_TTL_SECS, create_duels_dropdown, encode_png, extract_select_value, extract_tag_icons,
    fetch_skin, parse_duels_value, resolve_uuid, send_deferred_error, spawn_expiry_with_retain,
};

#[derive(Clone)]
struct AutoPreset {
    key: String,
    label: String,
    timestamp: DateTime<Utc>,
}


pub struct SessionDuelsCache {
    uuid: String,
    sender_id: u64,
    is_owner: bool,
    descriptions: HashMap<String, String>,
    markers: Vec<SessionMarker>,
    auto_presets: Vec<AutoPreset>,
    current_period: String,
    current_mode: DuelsView,
    render_data: SessionRenderData,
    last_interaction: Instant,
}


struct SessionRenderData {
    current_stats: DuelsStats,
    previous_stats: HashMap<String, (DuelsStats, SessionType, DateTime<Utc>)>,
    skin: Option<DynamicImage>,
    tag_icons: Vec<TagIcon>,
    snapshots: Vec<(DateTime<Utc>, DuelsWinstreakSnapshot)>,
    username: String,
    guild_info: Option<hypixel::GuildInfo>,
}


const PERIODS: [Period; 4] = [
    Period::Daily,
    Period::Weekly,
    Period::Monthly,
    Period::Yearly,
];


enum SessionError {
    PlayerNotFound,
    NoStats(String),
    ApiError,
}


enum SwitchResult {
    Ok(Vec<u8>, Vec<CreateComponent<'static>>),
    Expired,
    Ephemeral(Vec<u8>),
}


fn image_gallery() -> CreateComponent<'static> {
    CreateComponent::MediaGallery(CreateMediaGallery::new(vec![CreateMediaGalleryItem::new(
        CreateUnfurledMediaItem::new("attachment://session.png"),
    )]))
}


fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(ch, '*' | '_' | '~' | '`' | '|' | '>' | '\\' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}


fn view_display_name(view: &str) -> String {
    view.strip_prefix("marker:").unwrap_or(view).to_string()
}


fn format_duration(duration: Duration) -> String {
    let total_hours = duration.num_hours();
    if total_hours >= 24 {
        format!("{}d", duration.num_days())
    } else if total_hours >= 1 {
        let minutes = duration.num_minutes() % 60;
        if minutes > 0 {
            format!("{}h {}m", total_hours, minutes)
        } else {
            format!("{}h", total_hours)
        }
    } else {
        format!("{}m", duration.num_minutes().max(1))
    }
}


fn format_stats_delta(current: &DuelsStats, previous: &DuelsStats) -> String {
    let wins = current.overview.wins.saturating_sub(previous.overview.wins);
    let kills = current.overview.kills.saturating_sub(previous.overview.kills);
    let deaths = current.overview.deaths.saturating_sub(previous.overview.deaths);
    let kd = if deaths == 0 {
        kills as f64
    } else {
        kills as f64 / deaths as f64
    };
    format!("+{} wins, +{} kills, {:.2} kd", wins, kills, kd)
}


fn clone_session_type(session_type: &SessionType) -> SessionType {
    match session_type {
        SessionType::Custom(name) => SessionType::Custom(name.clone()),
        SessionType::Daily => SessionType::Daily,
        SessionType::Weekly => SessionType::Weekly,
        SessionType::Monthly => SessionType::Monthly,
        SessionType::Yearly => SessionType::Yearly,
    }
}


async fn send_ephemeral_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    content: &str,
) -> Result<()> {
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true),
            ),
        )
        .await?;
    Ok(())
}


fn extract_modal_field<'a>(modal: &'a ModalInteraction, field_name: &str) -> Option<&'a str> {
    modal.data.components.iter().find_map(|component| {
        if let Component::Label(label) = component {
            if let LabelComponent::InputText(input) = &label.component {
                if input.custom_id == field_name {
                    return input.value.as_deref();
                }
            }
        }
        None
    })
}


fn v2_update(
    components: Vec<CreateComponent<'static>>,
    png: Option<Vec<u8>>,
) -> CreateInteractionResponse<'static> {
    let mut all = Vec::with_capacity(components.len() + 1);
    if png.is_some() {
        all.push(image_gallery());
    }
    all.extend(components);

    let mut msg = CreateInteractionResponseMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(all);
    if let Some(png) = png {
        msg = msg.add_file(CreateAttachment::bytes(png, "session.png"));
    }
    CreateInteractionResponse::UpdateMessage(msg)
}


async fn update_original_components(
    ctx: &Context,
    component: &ComponentInteraction,
    components: Vec<CreateComponent<'static>>,
) {
    let mut all = Vec::with_capacity(components.len() + 1);
    all.push(image_gallery());
    all.extend(components);

    let edit = EditMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(all);
    let message = &component.message;
    let _ = ctx
        .http
        .edit_message(message.channel_id, message.id, &edit, Vec::new())
        .await;
}


fn evict_expired(cache: &mut HashMap<String, SessionDuelsCache>) {
    cache.retain(|_, value| value.last_interaction.elapsed().as_secs() <= CACHE_TTL_SECS);
}


fn build_session_components(
    cache_key: &str,
    uuid: &str,
    current_period: &str,
    current_mode: DuelsView,
    current_stats: &DuelsStats,
    descriptions: &HashMap<String, String>,
    markers: &[SessionMarker],
    auto_presets: &[AutoPreset],
    is_owner: bool,
) -> Vec<CreateComponent<'static>> {
    let mut container_rows = vec![
        CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(
            create_session_dropdown(
                cache_key,
                current_period,
                descriptions,
                markers,
                auto_presets,
                is_owner,
            ),
        )),
        CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(
            create_duels_dropdown("session_duels_mode", cache_key, current_mode, current_stats),
        )),
    ];

    if let Some(marker_name) = current_period.strip_prefix("marker:") {
        if is_owner {
            container_rows.push(CreateContainerComponent::ActionRow(
                CreateActionRow::Buttons(
                    vec![
                        CreateButton::new(format!(
                            "session_duels_mgmt_rename:{cache_key}:{uuid}:{marker_name}"
                        ))
                        .label("Rename")
                        .style(ButtonStyle::Primary),
                        CreateButton::new(format!(
                            "session_duels_mgmt_delete:{cache_key}:{uuid}:{marker_name}"
                        ))
                        .label("Delete")
                        .style(ButtonStyle::Danger),
                    ]
                    .into(),
                ),
            ));
        }
    }

    vec![CreateComponent::Container(CreateContainer::new(
        container_rows,
    ))]
}


fn create_session_dropdown(
    cache_key: &str,
    current: &str,
    descriptions: &HashMap<String, String>,
    markers: &[SessionMarker],
    auto_presets: &[AutoPreset],
    is_owner: bool,
) -> CreateSelectMenu<'static> {
    let mut options: Vec<CreateSelectMenuOption<'static>> = Vec::new();
    let now = Utc::now();

    for period in PERIODS {
        let key = period.key();
        let desc = descriptions
            .get(key)
            .map(String::as_str)
            .unwrap_or("No Data");
        let elapsed = now.signed_duration_since(period.last_reset(now));

        options.push(
            CreateSelectMenuOption::new(
                format!("{} ({})", period.label(), format_duration(elapsed)),
                format!("{key}:{cache_key}"),
            )
            .default_selection(current == key)
            .description(desc.to_string()),
        );

        if let Some((fp_key, fp_label)) = period.fixed_preset() {
            let fp_desc = descriptions
                .get(fp_key)
                .map(String::as_str)
                .unwrap_or("No Data");
            options.push(
                CreateSelectMenuOption::new(fp_label, format!("{fp_key}:{cache_key}"))
                    .default_selection(current == fp_key)
                    .description(fp_desc.to_string()),
            );
        }
    }

    for preset in auto_presets {
        let key = format!("preset:{}", preset.key);
        let age = format_duration(now.signed_duration_since(preset.timestamp));
        let mut option = CreateSelectMenuOption::new(
            format!("{} ({})", preset.label, age),
            format!("preset:{}:{}", cache_key, preset.key),
        )
        .default_selection(current == key);
        if let Some(desc) = descriptions.get(&key) {
            option = option.description(desc.clone());
        }
        options.push(option);
    }

    let remaining_slots = 25 - options.len() - if is_owner { 1 } else { 0 };
    for marker in markers.iter().take(remaining_slots) {
        let key = format!("marker:{}", marker.name);
        let age = format_duration(now.signed_duration_since(marker.snapshot_timestamp));
        let mut option = CreateSelectMenuOption::new(
            format!("\"{}\" ({})", sanitize(&marker.name), age),
            format!("marker:{}:{}", cache_key, marker.name),
        )
        .default_selection(current == key);
        if let Some(desc) = descriptions.get(&key) {
            option = option.description(desc.clone());
        }
        options.push(option);
    }

    if is_owner {
        options.push(
            CreateSelectMenuOption::new("Create New Bookmark", format!("create:{cache_key}"))
                .description("Bookmark your current stats"),
        );
    }

    let placeholder = PERIODS
        .iter()
        .find(|period| period.key() == current)
        .map(|period| period.label().to_string())
        .or_else(|| match current {
            "past_24h" => Some("Past 24 Hours".to_string()),
            "past_7d" => Some("Past 7 Days".to_string()),
            "past_30d" => Some("Past 30 Days".to_string()),
            _ => None,
        })
        .unwrap_or_else(|| {
            auto_presets
                .iter()
                .find(|preset| format!("preset:{}", preset.key) == current)
                .map(|preset| preset.label.clone())
                .unwrap_or_else(|| view_display_name(current))
        });

    CreateSelectMenu::new(
        "session_duels_switch",
        CreateSelectMenuKind::String {
            options: options.into(),
        },
    )
    .placeholder(placeholder)
}


fn render_selected_png(
    entry: &SessionDuelsCache,
    period_key: &str,
    mode: DuelsView,
) -> Option<Vec<u8>> {
    let (previous, session_type, timestamp) = entry.render_data.previous_stats.get(period_key)?;
    let winstreaks = duels_winstreaks::calculate(&entry.render_data.snapshots, mode);
    let image = render_duels_session(
        &entry.render_data.current_stats,
        previous,
        clone_session_type(session_type),
        *timestamp,
        None,
        mode,
        entry.render_data.skin.as_ref(),
        &winstreaks,
        &entry.render_data.tag_icons,
    );
    encode_png(&image).ok()
}


pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let player_input = command.data.options.first().and_then(|option| match &option.value {
        CommandDataOptionValue::SubCommand(options) => options
            .iter()
            .find(|option| option.name == "player")
            .and_then(|option| option.value.as_str())
            .map(|value| value.to_string()),
        _ => None,
    });
    let discord_id = command.user.id.get() as i64;

    let player = match player_input {
        Some(player) => player,
        None => match MemberRepository::new(data.db.pool())
            .get_by_discord_id(discord_id)
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

    let cache_key = command.id.to_string();
    let (defer_result, result) = tokio::join!(
        command.defer(&ctx.http),
        precompute_session(data, &player, discord_id)
    );
    defer_result?;

    match result {
        Ok(session_cache) => {
            let initial_period = PERIODS
                .iter()
                .map(|period| period.key())
                .find(|key| session_cache.render_data.previous_stats.contains_key(*key))
                .unwrap_or("daily");
            let initial_mode = session_cache.render_data.current_stats.default_view();
            let initial_png = render_selected_png(&session_cache, initial_period, initial_mode);
            let uuid = session_cache.uuid.clone();

            let is_owner = AccountRepository::new(data.db.pool())
                .is_owned_by(&uuid, discord_id)
                .await
                .unwrap_or(false);

            let components = build_session_components(
                &cache_key,
                &uuid,
                initial_period,
                initial_mode,
                &session_cache.render_data.current_stats,
                &session_cache.descriptions,
                &session_cache.markers,
                &session_cache.auto_presets,
                is_owner,
            );
            let expiry_key = cache_key.clone();

            {
                let mut cache = data.session_duels_images.lock().unwrap();
                evict_expired(&mut cache);
                let mut session_cache = session_cache;
                session_cache.current_period = initial_period.to_string();
                session_cache.current_mode = initial_mode;
                session_cache.is_owner = is_owner;
                cache.insert(cache_key, session_cache);
            }

            if let Some(png) = initial_png {
                let mut all = vec![image_gallery()];
                all.extend(components);

                command
                    .edit_response(
                        &ctx.http,
                        EditInteractionResponse::new()
                            .flags(MessageFlags::IS_COMPONENTS_V2)
                            .new_attachment(CreateAttachment::bytes(png, "session.png"))
                            .components(all),
                    )
                    .await?;

                spawn_expiry_with_retain(
                    ctx.http.clone(),
                    command.token.to_string(),
                    data.session_duels_images.clone(),
                    expiry_key,
                    |entry: &SessionDuelsCache| entry.last_interaction,
                    vec![image_gallery()],
                );
            } else {
                send_deferred_error(
                    ctx,
                    command,
                    "No Historical Data",
                    "No snapshot data available yet. Check back later!",
                )
                .await?;
            }
        }
        Err(SessionError::PlayerNotFound) => {
            send_deferred_error(
                ctx,
                command,
                "Player Not Found",
                &format!("Could not find player: {player}"),
            )
            .await?;
        }
        Err(SessionError::NoStats(username)) => {
            send_deferred_error(
                ctx,
                command,
                &format!("{username}'s Session Stats"),
                "This player has no Duels stats",
            )
            .await?;
        }
        Err(SessionError::ApiError) => {
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


pub async fn handle_switch(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let Some(value) = extract_select_value(component) else {
        return Ok(());
    };

    let parts: Vec<&str> = value.splitn(3, ':').collect();
    if parts.len() < 2 {
        return Ok(());
    }
    let (selection, cache_key, extra) = (parts[0], parts[1], parts.get(2).copied());

    if selection == "create" {
        return handle_create_bookmark(ctx, component, data, cache_key).await;
    }

    let period_key = match selection {
        "marker" => format!("marker:{}", extra.unwrap_or("")),
        "preset" => format!("preset:{}", extra.unwrap_or("")),
        _ => selection.to_string(),
    };

    match resolve_period_switch(data, cache_key, &period_key, component.user.id.get()) {
        SwitchResult::Ok(png, components) => {
            component
                .create_response(&ctx.http, v2_update(components, Some(png)))
                .await?;
        }
        SwitchResult::Expired => {
            super::disable_components(ctx, component).await?;
        }
        SwitchResult::Ephemeral(png) => {
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .add_file(CreateAttachment::bytes(png, "session.png"))
                            .ephemeral(true),
                    ),
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
        SwitchResult::Ok(png, components) => {
            component
                .create_response(&ctx.http, v2_update(components, Some(png)))
                .await?;
        }
        SwitchResult::Expired => {
            super::disable_components(ctx, component).await?;
        }
        SwitchResult::Ephemeral(png) => {
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .add_file(CreateAttachment::bytes(png, "session.png"))
                            .ephemeral(true),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}


async fn handle_create_bookmark(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    cache_key: &str,
) -> Result<()> {
    let discord_id = component.user.id.get() as i64;
    let timestamp = Utc::now();
    let name = timestamp.format("%b %-d, %Y").to_string();

    let (uuid, is_sender) = {
        let cache = data.session_duels_images.lock().unwrap();
        let Some(entry) = cache.get(cache_key) else {
            return Ok(());
        };
        (entry.uuid.clone(), entry.sender_id == component.user.id.get())
    };

    let is_owner = AccountRepository::new(data.db.pool())
        .is_owned_by(&uuid, discord_id)
        .await
        .unwrap_or(false);

    if !is_owner {
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("You can only create bookmarks for accounts linked to you.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    if let Err(error) = SessionRepository::new(data.db.pool())
        .create(&uuid, discord_id, &name, timestamp)
        .await
    {
        tracing::error!("Failed to create bookmark: {error}");
        return Ok(());
    }

    let snapshot_data = CacheRepository::new(data.db.pool())
        .get_snapshot_at(&uuid, timestamp)
        .await
        .ok()
        .flatten();

    let (components, png, ephemeral_png) = {
        let mut cache = data.session_duels_images.lock().unwrap();
        let Some(entry) = cache.get_mut(cache_key) else {
            return Ok(());
        };

        let to_stats = |value: Option<serde_json::Value>| -> Option<DuelsStats> {
            extract_duels_stats(
                &entry.render_data.username,
                &value?,
                entry.render_data.guild_info.clone(),
            )
        };

        let key = format!("marker:{name}");
        let mut bookmark_png = None;

        if let Some(previous_stats) = to_stats(snapshot_data) {
            let session_type = SessionType::Custom(name.to_string());
            entry.descriptions.insert(
                key.clone(),
                format_stats_delta(&entry.render_data.current_stats, &previous_stats),
            );
            entry.render_data.previous_stats.insert(
                key.clone(),
                (previous_stats, session_type, timestamp),
            );
            if is_sender {
                entry.current_period = key.clone();
            }

            bookmark_png = render_selected_png(entry, &key, entry.current_mode);
        }

        entry.markers.push(SessionMarker {
            id: 0,
            uuid: uuid.to_string(),
            discord_id,
            name: name.to_string(),
            snapshot_timestamp: timestamp,
            created_at: timestamp,
        });
        if is_sender {
            entry.last_interaction = Instant::now();
        }

        let png = render_selected_png(entry, &entry.current_period, entry.current_mode);
        let components = build_session_components(
            cache_key,
            &entry.uuid,
            &entry.current_period,
            entry.current_mode,
            &entry.render_data.current_stats,
            &entry.descriptions,
            &entry.markers,
            &entry.auto_presets,
            entry.is_owner,
        );
        (components, png, bookmark_png)
    };

    if is_sender {
        component
            .create_response(&ctx.http, v2_update(components, png))
            .await?;
    } else {
        let mut msg = CreateInteractionResponseMessage::new()
            .content("Bookmark created!")
            .ephemeral(true);
        if let Some(png) = ephemeral_png {
            msg = msg.add_file(CreateAttachment::bytes(png, "session.png"));
        }
        component
            .create_response(&ctx.http, CreateInteractionResponse::Message(msg))
            .await?;
        update_original_components(ctx, component, components).await;
    }

    Ok(())
}


pub async fn handle_mgmt_rename_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let rest = component
        .data
        .custom_id
        .strip_prefix("session_duels_mgmt_rename:")
        .unwrap_or("");
    let parts: Vec<&str> = rest.splitn(3, ':').collect();
    if parts.len() < 3 {
        return Ok(());
    }
    let (uuid, old_name) = (parts[1], parts[2]);

    let is_owner = AccountRepository::new(data.db.pool())
        .is_owned_by(uuid, component.user.id.get() as i64)
        .await
        .unwrap_or(false);

    if !is_owner {
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("You can only manage bookmarks for accounts linked to you.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    let input = CreateInputText::new(InputTextStyle::Short, "new_name")
        .placeholder("New session name")
        .min_length(1)
        .max_length(32);

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Modal(
                CreateModal::new(
                    format!("session_duels_rename_modal:{rest}"),
                    format!("Rename \"{old_name}\""),
                )
                .components(vec![CreateModalComponent::Label(
                    CreateLabel::input_text("New Name", input),
                )]),
            ),
        )
        .await?;

    Ok(())
}


pub async fn handle_rename_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let rest = modal
        .data
        .custom_id
        .strip_prefix("session_duels_rename_modal:")
        .unwrap_or("");
    let parts: Vec<&str> = rest.splitn(3, ':').collect();
    if parts.len() < 3 {
        return Ok(());
    }
    let (cache_key, uuid, old_name) = (parts[0], parts[1], parts[2]);

    let new_name = extract_modal_field(modal, "new_name").unwrap_or(old_name);
    let discord_id = modal.user.id.get() as i64;

    match SessionRepository::new(data.db.pool())
        .rename(uuid, discord_id, old_name, new_name)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            send_ephemeral_modal(ctx, modal, "Session not found").await?;
            return Ok(());
        }
        Err(error) => {
            tracing::error!("Failed to rename session: {error}");
            send_ephemeral_modal(ctx, modal, "Failed to rename session").await?;
            return Ok(());
        }
    }

    let (is_sender, cached_uuid) = {
        let cache = data.session_duels_images.lock().unwrap();
        let Some(entry) = cache.get(cache_key) else {
            return Ok(());
        };
        (entry.sender_id == modal.user.id.get(), entry.uuid.clone())
    };

    let fresh_markers = SessionRepository::new(data.db.pool())
        .list(&cached_uuid, discord_id)
        .await
        .unwrap_or_default();

    let (components, png) = {
        let mut cache = data.session_duels_images.lock().unwrap();
        let Some(entry) = cache.get_mut(cache_key) else {
            return Ok(());
        };

        let old_key = format!("marker:{old_name}");
        let new_key = format!("marker:{new_name}");

        if let Some(desc) = entry.descriptions.remove(&old_key) {
            entry.descriptions.insert(new_key.clone(), desc);
        }
        if let Some(previous) = entry.render_data.previous_stats.remove(&old_key) {
            entry.render_data.previous_stats.insert(
                new_key.clone(),
                (previous.0, SessionType::Custom(new_name.to_string()), previous.2),
            );
        }
        if is_sender && entry.current_period == old_key {
            entry.current_period = new_key;
        }
        entry.markers = fresh_markers;
        if is_sender {
            entry.last_interaction = Instant::now();
        }

        let png = render_selected_png(entry, &entry.current_period, entry.current_mode);
        let components = build_session_components(
            cache_key,
            &entry.uuid,
            &entry.current_period,
            entry.current_mode,
            &entry.render_data.current_stats,
            &entry.descriptions,
            &entry.markers,
            &entry.auto_presets,
            entry.is_owner,
        );
        (components, png)
    };

    if is_sender {
        modal
            .create_response(&ctx.http, v2_update(components, png))
            .await?;
    } else {
        send_ephemeral_modal(ctx, modal, "Bookmark renamed.").await?;
    }

    Ok(())
}


pub async fn handle_mgmt_delete_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let custom_id = component
        .data
        .custom_id
        .strip_prefix("session_duels_mgmt_delete:")
        .unwrap_or("");
    let parts: Vec<&str> = custom_id.splitn(3, ':').collect();
    if parts.len() < 3 {
        return Ok(());
    }
    let (cache_key, uuid, name) = (parts[0], parts[1], parts[2]);
    let discord_id = component.user.id.get() as i64;

    let is_owner = AccountRepository::new(data.db.pool())
        .is_owned_by(uuid, discord_id)
        .await
        .unwrap_or(false);

    if !is_owner {
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("You can only manage bookmarks for accounts linked to you.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    match SessionRepository::new(data.db.pool())
        .delete(uuid, discord_id, name)
        .await
    {
        Ok(false) | Err(_) => return Ok(()),
        Ok(true) => {}
    }

    let fresh_markers = SessionRepository::new(data.db.pool())
        .list(uuid, discord_id)
        .await
        .unwrap_or_default();

    let (is_sender, components, png) = {
        let mut cache = data.session_duels_images.lock().unwrap();
        let Some(entry) = cache.get_mut(cache_key) else {
            return Ok(());
        };

        let is_sender = entry.sender_id == component.user.id.get();
        let deleted_key = format!("marker:{name}");
        entry.descriptions.remove(&deleted_key);
        entry.render_data.previous_stats.remove(&deleted_key);

        if is_sender && entry.current_period == deleted_key {
            entry.current_period = PERIODS
                .iter()
                .map(|period| period.key().to_string())
                .find(|key| entry.render_data.previous_stats.contains_key(key))
                .unwrap_or_else(|| "daily".to_string());
        }
        entry.markers = fresh_markers;
        if is_sender {
            entry.last_interaction = Instant::now();
        }

        let png = render_selected_png(entry, &entry.current_period, entry.current_mode);
        let components = build_session_components(
            cache_key,
            &entry.uuid,
            &entry.current_period,
            entry.current_mode,
            &entry.render_data.current_stats,
            &entry.descriptions,
            &entry.markers,
            &entry.auto_presets,
            entry.is_owner,
        );
        (is_sender, components, png)
    };

    if is_sender {
        component
            .create_response(&ctx.http, v2_update(components, png))
            .await?;
    } else {
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Bookmark deleted.")
                        .ephemeral(true),
                ),
            )
            .await?;
        update_original_components(ctx, component, components).await;
    }

    Ok(())
}


fn resolve_period_switch(
    data: &Data,
    cache_key: &str,
    period_key: &str,
    user_id: u64,
) -> SwitchResult {
    let mut cache = data.session_duels_images.lock().unwrap();
    let Some(entry) = cache.get_mut(cache_key) else {
        return SwitchResult::Expired;
    };

    if entry.last_interaction.elapsed().as_secs() > CACHE_TTL_SECS {
        cache.remove(cache_key);
        return SwitchResult::Expired;
    }

    if entry.sender_id != user_id {
        return match render_selected_png(entry, period_key, entry.current_mode) {
            Some(png) => SwitchResult::Ephemeral(png),
            None => SwitchResult::Expired,
        };
    }

    let png = match render_selected_png(entry, period_key, entry.current_mode) {
        Some(png) => png,
        None => return SwitchResult::Expired,
    };
    entry.current_period = period_key.to_string();
    entry.last_interaction = Instant::now();

    let components = build_session_components(
        cache_key,
        &entry.uuid,
        &entry.current_period,
        entry.current_mode,
        &entry.render_data.current_stats,
        &entry.descriptions,
        &entry.markers,
        &entry.auto_presets,
        entry.is_owner,
    );
    SwitchResult::Ok(png, components)
}


fn resolve_mode_switch(
    data: &Data,
    cache_key: &str,
    view: DuelsView,
    user_id: u64,
) -> SwitchResult {
    let mut cache = data.session_duels_images.lock().unwrap();
    let Some(entry) = cache.get_mut(cache_key) else {
        return SwitchResult::Expired;
    };

    if entry.last_interaction.elapsed().as_secs() > CACHE_TTL_SECS {
        cache.remove(cache_key);
        return SwitchResult::Expired;
    }

    if entry.sender_id != user_id {
        return match render_selected_png(entry, &entry.current_period, view) {
            Some(png) => SwitchResult::Ephemeral(png),
            None => SwitchResult::Expired,
        };
    }

    let png = match render_selected_png(entry, &entry.current_period, view) {
        Some(png) => png,
        None => return SwitchResult::Expired,
    };
    entry.current_mode = view;
    entry.last_interaction = Instant::now();

    let components = build_session_components(
        cache_key,
        &entry.uuid,
        &entry.current_period,
        entry.current_mode,
        &entry.render_data.current_stats,
        &entry.descriptions,
        &entry.markers,
        &entry.auto_presets,
        entry.is_owner,
    );
    SwitchResult::Ok(png, components)
}


async fn precompute_session(
    data: &Data,
    player: &str,
    discord_id: i64,
) -> Result<SessionDuelsCache, SessionError> {
    let cached_uuid = resolve_uuid(data, player).await;
    let (resp, guild_result, skin_result) =
        fetch_player(data, player, cached_uuid.as_deref()).await?;

    let hypixel_data = resp.hypixel.ok_or(SessionError::PlayerNotFound)?;
    let username = resp.username.clone();
    let uuid = resp.uuid.clone();

    let guild_info = guild_result
        .ok()
        .flatten()
        .map(|guild| super::to_guild_info(&guild));
    let skin_image = skin_result.map(|skin| skin.data);
    let current_stats = extract_duels_stats(&username, &hypixel_data, guild_info.clone())
        .ok_or_else(|| SessionError::NoStats(username.clone()))?;

    let cache_repo = CacheRepository::new(data.db.pool());
    let (session_snapshots, markers, auto_presets, ws_snapshots) = {
        let (s, m, a) = fetch_snapshots(data, &uuid, discord_id).await;
        let ws = cache_repo
            .get_all_snapshots_mapped(&uuid, extract_duels_winstreak_snapshot)
            .await;
        (s, m, a, ws)
    };

    let to_stats = |value: Option<serde_json::Value>| -> Option<DuelsStats> {
        extract_duels_stats(&username, &value?, guild_info.clone())
    };

    let tags = extract_tag_icons(&resp.tags);
    let (previous_stats, descriptions, marker_list) = build_previous_views(
        &current_stats,
        session_snapshots,
        &markers,
        &auto_presets,
        to_stats,
    );

    Ok(SessionDuelsCache {
        uuid,
        sender_id: discord_id as u64,
        is_owner: false,
        descriptions,
        markers: marker_list,
        auto_presets,
        current_period: "daily".to_string(),
        current_mode: DuelsView::Overall,
        render_data: SessionRenderData {
            current_stats,
            previous_stats,
            skin: skin_image,
            tag_icons: tags,
            snapshots: ws_snapshots.unwrap_or_default(),
            username,
            guild_info,
        },
        last_interaction: Instant::now(),
    })
}


async fn fetch_player(
    data: &Data,
    player: &str,
    cached_uuid: Option<&str>,
) -> Result<
    (
        crate::api::PlayerStatsResponse,
        Result<Option<crate::api::GuildResponse>, crate::api::ApiError>,
        Option<clients::SkinImage>,
    ),
    SessionError,
> {
    match cached_uuid {
        Some(uuid) => {
            let (api, guild, skin) = tokio::join!(
                data.api.get_player_stats(player),
                data.api.get_guild(uuid, Some("player")),
                data.skin_provider.fetch(uuid),
            );
            let resp = api.map_err(map_api_error)?;
            if resp.uuid == uuid {
                return Ok((resp, guild, skin));
            }
            let (guild, skin) = tokio::join!(
                data.api.get_guild(&resp.uuid, Some("player")),
                fetch_skin(data, &resp.uuid, resp.skin_url.as_deref(), resp.slim),
            );
            Ok((resp, guild, skin))
        }
        None => {
            let resp = data
                .api
                .get_player_stats(player)
                .await
                .map_err(map_api_error)?;
            let (guild, skin) = tokio::join!(
                data.api.get_guild(&resp.uuid, Some("player")),
                fetch_skin(data, &resp.uuid, resp.skin_url.as_deref(), resp.slim),
            );
            Ok((resp, guild, skin))
        }
    }
}


fn map_api_error(error: crate::api::ApiError) -> SessionError {
    match error {
        crate::api::ApiError::NotFound => SessionError::PlayerNotFound,
        other => {
            tracing::error!("Internal API error: {other}");
            SessionError::ApiError
        }
    }
}


async fn fetch_snapshots(
    data: &Data,
    uuid: &str,
    discord_id: i64,
) -> (
    Vec<Option<(DateTime<Utc>, serde_json::Value)>>,
    Vec<SessionMarker>,
    Vec<AutoPreset>,
) {
    let session_repo = SessionRepository::new(data.db.pool());
    let cache_repo = CacheRepository::new(data.db.pool());
    let now = Utc::now();

    let (mut markers, auto_presets) = tokio::join!(
        async { session_repo.list(uuid, discord_id).await.unwrap_or_default() },
        detect_auto_presets(&cache_repo, uuid),
    );

    if markers.is_empty() {
        if let Ok(marker) = session_repo.create(uuid, discord_id, "main", now).await {
            markers.push(marker);
        }
    }

    let mut timestamps: Vec<DateTime<Utc>> = PERIODS.iter().map(|period| period.last_reset(now)).collect();
    for period in PERIODS {
        if period.fixed_preset().is_some() {
            timestamps.push(now - period.duration());
        }
    }
    for marker in &markers {
        timestamps.push(marker.snapshot_timestamp);
    }
    for preset in &auto_presets {
        timestamps.push(preset.timestamp);
    }

    let snapshots = cache_repo
        .get_snapshots_at_times(uuid, &timestamps)
        .await
        .unwrap_or_else(|_| vec![None; timestamps.len()]);

    (snapshots, markers, auto_presets)
}


fn build_previous_views(
    current_stats: &DuelsStats,
    snapshots: Vec<Option<(DateTime<Utc>, serde_json::Value)>>,
    markers: &[SessionMarker],
    auto_presets: &[AutoPreset],
    to_stats: impl Fn(Option<serde_json::Value>) -> Option<DuelsStats>,
) -> (
    HashMap<String, (DuelsStats, SessionType, DateTime<Utc>)>,
    HashMap<String, String>,
    Vec<SessionMarker>,
) {
    let now = Utc::now();
    let mut previous_stats = HashMap::new();
    let mut descriptions = HashMap::new();
    let mut snapshot_iter = snapshots.into_iter();

    let mut register_view =
        |key: String, previous: DuelsStats, session_type: SessionType, timestamp: DateTime<Utc>| {
            descriptions.insert(key.clone(), format_stats_delta(current_stats, &previous));
            previous_stats.insert(key, (previous, session_type, timestamp));
        };

    for period in PERIODS {
        let target_time = period.last_reset(now);
        if let Some(previous) = to_stats(snapshot_iter.next().flatten().map(|(_, value)| value)) {
            register_view(
                period.key().to_string(),
                previous,
                match period {
                    Period::Daily => SessionType::Daily,
                    Period::Weekly => SessionType::Weekly,
                    Period::Monthly => SessionType::Monthly,
                    Period::Yearly => SessionType::Yearly,
                },
                target_time,
            );
        }
    }

    for period in PERIODS {
        let Some((fp_key, fp_label)) = period.fixed_preset() else {
            continue;
        };
        let target_time = now - period.duration();
        if let Some(previous) = to_stats(snapshot_iter.next().flatten().map(|(_, value)| value)) {
            register_view(
                fp_key.to_string(),
                previous,
                SessionType::Custom(fp_label.to_string()),
                target_time,
            );
        }
    }

    for marker in markers {
        if let Some(previous) = to_stats(snapshot_iter.next().flatten().map(|(_, value)| value)) {
            register_view(
                format!("marker:{}", marker.name),
                previous,
                SessionType::Custom(marker.name.clone()),
                marker.snapshot_timestamp,
            );
        }
    }

    for preset in auto_presets {
        if let Some(previous) = to_stats(snapshot_iter.next().flatten().map(|(_, value)| value)) {
            register_view(
                format!("preset:{}", preset.key),
                previous,
                SessionType::Custom(preset.label.clone()),
                preset.timestamp,
            );
        }
    }

    (previous_stats, descriptions, markers.to_vec())
}


struct SnapshotFields {
    losses: u64,
}


async fn detect_auto_presets(cache_repo: &CacheRepository<'_>, uuid: &str) -> Vec<AutoPreset> {
    let snapshots = cache_repo
        .get_all_snapshots_mapped(uuid, |value| {
            let duels = value.get("stats")?.get("Duels")?;
            Some(SnapshotFields {
                losses: duels.get("losses").and_then(|entry| entry.as_u64()).unwrap_or(0),
            })
        })
        .await
        .unwrap_or_default();

    if snapshots.is_empty() {
        return vec![];
    }

    let mut presets = Vec::new();
    if let Some(timestamp) = snapshots.windows(2).rev().find_map(|window| {
        let (_, before) = &window[0];
        let (timestamp, after) = &window[1];
        (after.losses > before.losses).then_some(*timestamp)
    }) {
        presets.push(AutoPreset {
            key: "since_loss".to_string(),
            label: "Since Last Loss".to_string(),
            timestamp,
        });
    }

    presets
}
