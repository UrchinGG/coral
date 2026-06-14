use anyhow::Result;
use serenity::all::*;

use super::{state::*, *};
use crate::framework::Data;

pub async fn handle_add_player(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let submitter_id = parse_submitter_id(&component.data.custom_id).unwrap_or(0);
    let input = CreateInputText::new(InputTextStyle::Short, "player")
        .placeholder("Minecraft username")
        .min_length(1)
        .max_length(16);

    let modal = CreateModal::new(
        format!("review_addplayer_name:{submitter_id}"),
        "Add Player",
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Player Name",
        input,
    ))]);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_addplayer_name_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;

    let player_name = extract_modal_value(modal, "player");
    let Ok(info) = data.api.resolve(&player_name).await else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .content(format!("Could not find a player named `{player_name}`")),
            )
            .await?;
        return Ok(());
    };
    let (resolved_name, resolved_uuid) = (info.username, info.uuid);

    let channel_id = modal.channel_id;
    let Some(builder_msg) = find_builder_message(ctx, channel_id).await else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not find the submission message"),
            )
            .await?;
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not parse submission state"),
            )
            .await?;
        return Ok(());
    };

    if state.players.iter().any(|p| p.uuid == resolved_uuid) {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new()
                    .content(format!("`{resolved_name}` is already in this submission")),
            )
            .await?;
        return Ok(());
    }

    state.pending_add = Some(PendingAdd {
        identifier: resolved_uuid,
        username: resolved_name,
    });

    update_builder(ctx, data, channel_id, &builder_msg, &state).await?;
    let _ = modal.delete_response(&ctx.http).await;
    Ok(())
}

pub async fn handle_pending_tag_select(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let tag_type = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().map(|s| s.as_str()).unwrap_or("")
        }
        _ => return Ok(()),
    };
    if !super::REVIEW_TAGS.contains(&tag_type) {
        return Ok(());
    }

    let custom_id = component
        .data
        .custom_id
        .strip_prefix("review_pending_tag:")
        .unwrap_or("");
    let parts: Vec<&str> = custom_id.rsplitn(2, ':').collect();
    let submitter_id = parts.first().unwrap_or(&"0");
    let identifier = parts.get(1).unwrap_or(&"");

    let reason_input = CreateInputText::new(InputTextStyle::Short, "reason")
        .placeholder("Reason for this tag")
        .min_length(1)
        .max_length(120);

    let modal = CreateModal::new(
        format!("review_addplayer_reason:{identifier}:{tag_type}:{submitter_id}"),
        "Add Player \u{2014} Reason",
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Reason",
        reason_input,
    ))]);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_addplayer_reason_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;

    let custom_id = modal
        .data
        .custom_id
        .strip_prefix("review_addplayer_reason:")
        .unwrap_or("");
    let parts: Vec<&str> = custom_id.rsplitn(3, ':').collect();
    let tag_type = parts.get(1).unwrap_or(&"").to_string();
    let identifier = parts.get(2).unwrap_or(&"").to_string();

    if !super::REVIEW_TAGS.contains(&tag_type.as_str()) {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content(format!("Invalid tag type: `{tag_type}`")),
            )
            .await?;
        return Ok(());
    }

    let reason = extract_modal_value(modal, "reason");
    let username = data
        .api
        .resolve(&identifier)
        .await
        .map(|r| r.username)
        .unwrap_or_else(|_| identifier.clone());
    let uuid = identifier.clone();

    let channel_id = modal.channel_id;
    let Some(builder_msg) = find_builder_message(ctx, channel_id).await else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not find the submission message"),
            )
            .await?;
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not parse submission state"),
            )
            .await?;
        return Ok(());
    };

    state.players.push(PlayerEntry {
        username,
        uuid,
        tag_type,
        reason,
        status: PlayerStatus::Pending,
        review_note: None,
        evidence: Vec::new(),
        accept_votes: Vec::new(),
        reject_votes: Vec::new(),
    });

    update_builder(ctx, data, channel_id, &builder_msg, &state).await?;
    let _ = modal.delete_response(&ctx.http).await;
    Ok(())
}

pub async fn handle_remove_player(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let (player_idx, _) = parse_component_ids(&component.data.custom_id);
    let channel_id = component.channel_id;

    let Some(builder_msg) = find_builder_message(ctx, channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        return Ok(());
    };

    if player_idx < state.players.len() {
        state.players.remove(player_idx);
    }

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, channel_id, &builder_msg, &state).await
}

pub async fn handle_edit_tag(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let (player_idx, _) = parse_component_ids(&component.data.custom_id);
    let Some(message) = find_builder_message(ctx, component.channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&message) else {
        return Ok(());
    };

    state.editing = Some(player_idx);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, component.channel_id, &message, &state).await
}

pub async fn handle_edit_done(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let Some(message) = find_builder_message(ctx, component.channel_id).await else {
        return Ok(());
    };
    let Some(state) = parse_state_from_message(&message) else {
        return Ok(());
    };

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, component.channel_id, &message, &state).await
}

pub async fn handle_tag_select_edit(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let tag_type = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            values.first().map(|s| s.as_str()).unwrap_or("")
        }
        _ => return Ok(()),
    };
    if !super::REVIEW_TAGS.contains(&tag_type) {
        return Ok(());
    }

    let (player_idx, _) = parse_component_ids(&component.data.custom_id);
    let channel_id = component.channel_id;
    let Some(builder_msg) = find_builder_message(ctx, channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        return Ok(());
    };
    if let Some(player) = state.players.get_mut(player_idx) {
        player.tag_type = tag_type.to_string();
    }
    state.editing = Some(player_idx);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, channel_id, &builder_msg, &state).await
}

pub async fn handle_edit_reason(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let (player_idx, submitter_id) = parse_component_ids(&component.data.custom_id);
    let current_reason = parse_state_from_message(&component.message)
        .and_then(|s| s.players.get(player_idx).map(|p| p.reason.clone()))
        .unwrap_or_default();

    let reason_input = CreateInputText::new(InputTextStyle::Short, "reason")
        .placeholder("Reason for this tag")
        .value(current_reason)
        .min_length(1)
        .max_length(120);

    let modal = CreateModal::new(
        format!("review_edit_reason_modal:{player_idx}:{submitter_id}"),
        "Edit Reason",
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Reason",
        reason_input,
    ))]);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_edit_reason_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    modal.defer_ephemeral(&ctx.http).await?;

    let player_idx: usize = modal
        .data
        .custom_id
        .strip_prefix("review_edit_reason_modal:")
        .and_then(|s| s.split(':').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let reason = extract_modal_value(modal, "reason");
    let channel_id = modal.channel_id;

    let Some(builder_msg) = find_builder_message(ctx, channel_id).await else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not find the submission message"),
            )
            .await?;
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Could not parse submission state"),
            )
            .await?;
        return Ok(());
    };
    let Some(player) = state.players.get_mut(player_idx) else {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("Player not found"),
            )
            .await?;
        return Ok(());
    };

    player.reason = reason;
    state.editing = Some(player_idx);

    update_builder(ctx, data, channel_id, &builder_msg, &state).await?;
    let _ = modal.delete_response(&ctx.http).await;
    Ok(())
}

pub async fn handle_edit_submitted(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let Some(message) = find_builder_message(ctx, component.channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&message) else {
        return Ok(());
    };

    state.submitted = false;
    state.reopened = true;
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, component.channel_id, &message, &state).await
}
