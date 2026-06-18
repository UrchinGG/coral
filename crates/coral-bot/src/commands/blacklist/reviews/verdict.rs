use anyhow::Result;
use blacklist::{EMOTE_ADDTAG, lookup as lookup_tag};
use coral_redis::BlacklistEvent;
use database::{BlacklistRepository, MemberRepository};
use serenity::all::*;

use super::{builder::*, state::*, *};
use crate::{framework::Data, utils::text};

async fn send_in_thread(
    ctx: &Context,
    channel_id: GenericChannelId,
    msg: &CreateMessage<'_>,
    files: Vec<CreateAttachment<'_>>,
) {
    let _ = ctx.http.send_message(channel_id, files, msg).await;
}

#[allow(clippy::too_many_arguments)]
async fn announce_vote(
    ctx: &Context,
    data: &Data,
    channel_id: GenericChannelId,
    player_index: usize,
    voter_id: u64,
    vote_type: &str,
    tag_type: &str,
    username: &str,
    changed: bool,
) {
    let key = (channel_id.get(), player_index, voter_id);
    let existing = data.vote_messages.lock().unwrap().get(&key).copied();
    if let Some(mid) = existing {
        let edit = EditMessage::new()
            .flags(MessageFlags::IS_COMPONENTS_V2)
            .components(build_vote_components(
                voter_id, vote_type, tag_type, username, changed,
            ));
        if ctx
            .http
            .edit_message(
                channel_id,
                MessageId::new(mid),
                &edit,
                Vec::<CreateAttachment>::new(),
            )
            .await
            .is_ok()
        {
            return;
        }
    }
    let msg = CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(build_vote_components(
            voter_id, vote_type, tag_type, username, changed,
        ));
    if let Ok(sent) = ctx
        .http
        .send_message(channel_id, Vec::<CreateAttachment>::new(), &msg)
        .await
    {
        data.vote_messages
            .lock()
            .unwrap()
            .insert(key, sent.id.get());
    }
}

fn load_player_votes(
    data: &Data,
    thread_id: u64,
    player_index: usize,
    parsed_accepts: &[u64],
    parsed_rejects: &[u64],
) -> (Vec<u64>, Vec<u64>) {
    let map = data.pending_review_votes.lock().unwrap();
    if let Some(thread) = map.get(&thread_id) {
        if let Some((a, r)) = thread.get(&player_index) {
            return (a.clone(), r.clone());
        }
    }
    (parsed_accepts.to_vec(), parsed_rejects.to_vec())
}

fn record_player_vote(
    data: &Data,
    thread_id: u64,
    player_index: usize,
    voter_id: u64,
    accept: bool,
    current_accepts: &[u64],
    current_rejects: &[u64],
) -> (Vec<u64>, Vec<u64>) {
    let mut map = data.pending_review_votes.lock().unwrap();
    let thread = map.entry(thread_id).or_default();
    let entry = thread
        .entry(player_index)
        .or_insert_with(|| (current_accepts.to_vec(), current_rejects.to_vec()));
    if accept {
        entry.1.retain(|&id| id != voter_id);
        if !entry.0.contains(&voter_id) {
            entry.0.push(voter_id);
        }
    } else {
        entry.0.retain(|&id| id != voter_id);
        if !entry.1.contains(&voter_id) {
            entry.1.push(voter_id);
        }
    }
    (entry.0.clone(), entry.1.clone())
}

fn cleanup_review_votes(data: &Data, thread_id: u64) {
    data.pending_review_votes.lock().unwrap().remove(&thread_id);
    data.vote_messages
        .lock()
        .unwrap()
        .retain(|(t, _, _), _| *t != thread_id);
}

pub async fn handle_submit(
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
        return send_vote_error(ctx, component, "Could not parse submission state").await;
    };

    if state.players.is_empty() {
        return send_vote_error(ctx, component, "Add at least one player before submitting").await;
    }
    if !state.players.iter().any(|p| !p.evidence.is_empty()) {
        return send_vote_error(
            ctx,
            component,
            "Add at least one piece of evidence (replay or attachment) before submitting",
        )
        .await;
    }

    state.submitted = true;

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, component.channel_id, &message, &state).await?;

    let op_id = MessageId::new(component.channel_id.get());
    if component.message.id != op_id {
        let _ = ctx
            .http
            .delete_message(component.channel_id, component.message.id, None)
            .await;
    }

    let tags = resolve_forum_tags(ctx, data).await;
    let mut tag_ids = Vec::new();
    if let Some(id) = tags.pending {
        tag_ids.push(id);
    }
    let _ = set_forum_tags(ctx, thread_id(component.channel_id), &tag_ids).await;
    Ok(())
}

pub async fn handle_approve(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (player_index, submitter_id) = parse_component_ids(&component.data.custom_id);
    let discord_id = component.user.id.get();
    let rank = super::super::tag::get_rank(data, discord_id).await?;

    if rank < data.vote_min_rank {
        return send_vote_error(
            ctx,
            component,
            "You do not have permission to review submissions",
        )
        .await;
    }
    if discord_id == submitter_id {
        return send_vote_error(ctx, component, "You cannot review your own submission").await;
    }

    let Some(message) = find_builder_message(ctx, component.channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&message) else {
        return Ok(());
    };
    let Some(player) = state.players.get(player_index) else {
        return Ok(());
    };

    if player.status != PlayerStatus::Pending {
        return send_vote_error(ctx, component, "This player has already been reviewed").await;
    }

    let thread_key = component.channel_id.get();
    let (existing_accepts, existing_rejects) = load_player_votes(
        data,
        thread_key,
        player_index,
        &state.players[player_index].accept_votes,
        &state.players[player_index].reject_votes,
    );
    state.players[player_index].accept_votes = existing_accepts;
    state.players[player_index].reject_votes = existing_rejects;

    if state.players[player_index]
        .accept_votes
        .contains(&discord_id)
    {
        return send_vote_error(
            ctx,
            component,
            "You have already voted to accept this player",
        )
        .await;
    }
    let changing_vote = state.players[player_index]
        .reject_votes
        .contains(&discord_id);

    let is_staff = rank >= crate::framework::AccessRank::Helper;

    if !is_staff {
        let (new_accepts, new_rejects) = record_player_vote(
            data,
            thread_key,
            player_index,
            discord_id,
            true,
            &state.players[player_index].accept_votes,
            &state.players[player_index].reject_votes,
        );
        state.players[player_index].accept_votes = new_accepts;
        state.players[player_index].reject_votes = new_rejects;
        let unanimous = state.players[player_index].reject_votes.is_empty()
            && state.players[player_index].accept_votes.len() >= super::ACCEPT_THRESHOLD;

        if !unanimous {
            let tag_type = state.players[player_index].tag_type.clone();
            let username = state.players[player_index].username.clone();
            component
                .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
                .await?;
            update_builder(ctx, data, component.channel_id, &message, &state).await?;
            announce_vote(
                ctx,
                data,
                component.channel_id.into(),
                player_index,
                discord_id,
                "accept",
                &tag_type,
                &username,
                changing_vote,
            )
            .await;
            return Ok(());
        }
    }

    let player = &state.players[player_index];
    let player_uuid = player.uuid.clone();
    let player_tag_type = player.tag_type.clone();
    let player_username = player.username.clone();
    let player_reason = player.reason.clone();
    let media_urls = extract_media_urls_from_message(&message, player_index);

    if !super::REVIEW_TAGS.contains(&player_tag_type.as_str()) {
        return send_vote_error(ctx, component, "Invalid tag type in submission").await;
    }

    let repo = BlacklistRepository::new(data.db.pool());
    let reviewed_by: Vec<i64> = if is_staff {
        vec![discord_id as i64]
    } else {
        state.players[player_index]
            .accept_votes
            .iter()
            .map(|&id| id as i64)
            .collect()
    };

    let existing_tags = repo.get_active_tags(&player_uuid).await?;
    let new_priority = lookup_tag(&player_tag_type)
        .map(|d| d.priority)
        .unwrap_or(0);
    if let Some(conflict) = existing_tags.iter().find(|t| {
        lookup_tag(t.tag_type.as_deref().unwrap_or(""))
            .map(|d| d.priority)
            .unwrap_or(0)
            == new_priority
    }) {
        repo.remove_event(
            &player_uuid,
            conflict.tag_type.as_deref().unwrap_or(""),
            Some(discord_id as i64),
        )
        .await?;
    }

    let reviewed_by_slice = if reviewed_by.is_empty() {
        None
    } else {
        Some(reviewed_by.as_slice())
    };
    let will_confirm =
        !media_urls.is_empty() && CONFIRMABLE_TAGS.contains(&player_tag_type.as_str());
    let stored_type = if will_confirm {
        "confirmed_cheater".to_string()
    } else {
        player_tag_type.clone()
    };

    let blocking = vec![stored_type.clone()];
    let outcome = repo
        .add_event(
            &player_uuid,
            &stored_type,
            &player_reason,
            false,
            None,
            reviewed_by_slice,
            Some(submitter_id as i64),
            &blocking,
        )
        .await?;
    let tag_id = match outcome {
        database::AddOutcome::Inserted(id) => id,
        database::AddOutcome::Conflict(_) => {
            return send_vote_error(
                ctx,
                component,
                "Could not apply tag — a conflicting tag was added concurrently",
            )
            .await;
        }
    };

    let guild_id = component.guild_id.map(|g| g.get()).unwrap_or(0);
    let review_url = format!(
        "https://discord.com/channels/{}/{}",
        guild_id,
        component.channel_id.get(),
    );
    let reviewer_names: Vec<String> = futures::future::join_all(
        reviewed_by
            .iter()
            .map(|&id| super::super::channel::get_username(ctx, id as u64)),
    )
    .await;

    if will_confirm {
        if let Err(e) = super::super::evidence::create_evidence_from_review(
            ctx,
            data,
            &player_uuid,
            &player_username,
            &player_reason,
            &media_urls,
            Some(&review_url),
            &reviewer_names,
        )
        .await
        {
            tracing::error!("Failed to create evidence post: {e:#}");
        }
    }

    let tags = repo.get_active_tags(&player_uuid).await?;
    if tags.iter().any(|t| t.id == tag_id) {
        data.event_publisher
            .publish(&BlacklistEvent::TagAdded {
                uuid: player_uuid.clone(),
                tag_id,
                added_by: submitter_id as i64,
                silent: false,
                review_url: Some(review_url.clone()),
            })
            .await;
    }

    let member_repo = MemberRepository::new(data.db.pool());
    if let Err(e) = member_repo
        .increment_accepted_tags(submitter_id as i64)
        .await
    {
        tracing::error!("Failed to increment accepted tags for {submitter_id}: {e}");
    }

    let accurate_ids: Vec<i64> = state.players[player_index]
        .accept_votes
        .iter()
        .map(|&id| id as i64)
        .collect();
    if !accurate_ids.is_empty() {
        if let Err(e) = member_repo.increment_accurate_verdicts(&accurate_ids).await {
            tracing::error!("Failed to increment accurate verdicts: {e}");
        }
    }

    state.players[player_index].status = PlayerStatus::Approved;
    state.players[player_index].tag_type = stored_type.clone();
    state.players[player_index].reviewer_names = reviewer_names;
    state.players[player_index].accept_votes.clear();
    state.players[player_index].reject_votes.clear();

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, component.channel_id, &message, &state).await?;

    let all_resolved =
        finalize_thread_if_resolved(ctx, data, thread_id(component.channel_id), &state).await?;

    let msg = build_verdict_message(
        discord_id,
        is_staff,
        &state.players[player_index],
        Some(stored_type.as_str()),
        all_resolved.then_some(&state),
    );
    let face = verdict_face(data, &state.players[player_index].uuid).await;
    send_in_thread(ctx, component.channel_id.into(), &msg, vec![face]).await;
    if all_resolved {
        close_thread(ctx, thread_id(component.channel_id)).await;
    }
    Ok(())
}

pub async fn handle_reject(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (player_index, submitter_id) = parse_component_ids(&component.data.custom_id);
    let discord_id = component.user.id.get();
    let rank = super::super::tag::get_rank(data, discord_id).await?;

    if rank < data.vote_min_rank {
        return send_vote_error(
            ctx,
            component,
            "You do not have permission to review submissions",
        )
        .await;
    }
    if discord_id == submitter_id {
        return send_vote_error(ctx, component, "You cannot review your own submission").await;
    }

    let is_staff = rank >= crate::framework::AccessRank::Helper;
    if is_staff {
        let reason_input = CreateInputText::new(InputTextStyle::Short, "reason")
            .placeholder("Why is this submission being rejected?")
            .min_length(1)
            .max_length(30);

        let modal = CreateModal::new(
            format!("review_reject_modal:{player_index}:{submitter_id}"),
            "Reject Submission",
        )
        .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
            "Rejection Reason",
            reason_input,
        ))]);

        component
            .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
            .await?;
        return Ok(());
    }

    let Some(message) = find_builder_message(ctx, component.channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&message) else {
        return Ok(());
    };
    let Some(player) = state.players.get(player_index) else {
        return Ok(());
    };

    if player.status != PlayerStatus::Pending {
        return send_vote_error(ctx, component, "This player has already been reviewed").await;
    }

    let thread_key = component.channel_id.get();
    let (existing_accepts, existing_rejects) = load_player_votes(
        data,
        thread_key,
        player_index,
        &state.players[player_index].accept_votes,
        &state.players[player_index].reject_votes,
    );
    state.players[player_index].accept_votes = existing_accepts;
    state.players[player_index].reject_votes = existing_rejects;

    if state.players[player_index]
        .reject_votes
        .contains(&discord_id)
    {
        return send_vote_error(
            ctx,
            component,
            "You have already voted to reject this player",
        )
        .await;
    }
    let changing_vote = state.players[player_index]
        .accept_votes
        .contains(&discord_id);

    let (new_accepts, new_rejects) = record_player_vote(
        data,
        thread_key,
        player_index,
        discord_id,
        false,
        &state.players[player_index].accept_votes,
        &state.players[player_index].reject_votes,
    );
    state.players[player_index].accept_votes = new_accepts;
    state.players[player_index].reject_votes = new_rejects;
    let unanimous = state.players[player_index].accept_votes.is_empty()
        && state.players[player_index].reject_votes.len() >= super::REJECT_THRESHOLD;

    if !unanimous {
        let tag_type = state.players[player_index].tag_type.clone();
        let username = state.players[player_index].username.clone();
        component
            .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
            .await?;
        update_builder(ctx, data, component.channel_id, &message, &state).await?;
        announce_vote(
            ctx,
            data,
            component.channel_id.into(),
            player_index,
            discord_id,
            "reject",
            &tag_type,
            &username,
            changing_vote,
        )
        .await;
        return Ok(());
    }

    let member_repo = MemberRepository::new(data.db.pool());
    if let Err(e) = member_repo
        .increment_rejected_tags(submitter_id as i64)
        .await
    {
        tracing::error!("Failed to increment rejected tags for {submitter_id}: {e}");
    }

    let accurate_ids: Vec<i64> = state.players[player_index]
        .reject_votes
        .iter()
        .map(|&id| id as i64)
        .collect();
    if !accurate_ids.is_empty() {
        if let Err(e) = member_repo.increment_accurate_verdicts(&accurate_ids).await {
            tracing::error!("Failed to increment accurate verdicts: {e}");
        }
    }

    state.players[player_index].status = PlayerStatus::Rejected;
    state.players[player_index].accept_votes.clear();
    state.players[player_index].reject_votes.clear();

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    update_builder(ctx, data, component.channel_id, &message, &state).await?;

    let all_resolved =
        finalize_thread_if_resolved(ctx, data, thread_id(component.channel_id), &state).await?;

    let msg = build_verdict_message(
        discord_id,
        false,
        &state.players[player_index],
        None,
        all_resolved.then_some(&state),
    );
    send_in_thread(ctx, component.channel_id.into(), &msg, Vec::new()).await;
    if all_resolved {
        close_thread(ctx, thread_id(component.channel_id)).await;
    }
    Ok(())
}

pub async fn handle_reject_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let custom_id = modal
        .data
        .custom_id
        .strip_prefix("review_reject_modal:")
        .unwrap_or("");
    let parts: Vec<&str> = custom_id.split(':').collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let player_index: usize = parts[0].parse().unwrap_or(0);
    let submitter_id: u64 = parts[1].parse().unwrap_or(0);
    let reason = extract_modal_value(modal, "reason");
    let discord_id = modal.user.id.get();

    modal.defer_ephemeral(&ctx.http).await?;

    let channel_id = modal.channel_id;
    let Some(builder_msg) = find_builder_message(ctx, channel_id).await else {
        return Ok(());
    };
    let Some(mut state) = parse_state_from_message(&builder_msg) else {
        return Ok(());
    };
    let Some(player) = state.players.get(player_index) else {
        return Ok(());
    };

    if player.status != PlayerStatus::Pending {
        modal
            .edit_response(
                &ctx.http,
                EditInteractionResponse::new().content("This player has already been reviewed"),
            )
            .await?;
        return Ok(());
    }

    let member_repo = MemberRepository::new(data.db.pool());
    if let Err(e) = member_repo
        .increment_rejected_tags(submitter_id as i64)
        .await
    {
        tracing::error!("Failed to increment rejected tags for {submitter_id}: {e}");
    }

    let accurate_ids: Vec<i64> = state.players[player_index]
        .reject_votes
        .iter()
        .map(|&id| id as i64)
        .collect();
    if !accurate_ids.is_empty() {
        if let Err(e) = member_repo.increment_accurate_verdicts(&accurate_ids).await {
            tracing::error!("Failed to increment accurate verdicts: {e}");
        }
    }

    state.players[player_index].status = PlayerStatus::Rejected;
    state.players[player_index].review_note = Some(reason.clone());
    state.players[player_index].accept_votes.clear();
    state.players[player_index].reject_votes.clear();

    update_builder(ctx, data, channel_id, &builder_msg, &state).await?;

    let all_resolved =
        finalize_thread_if_resolved(ctx, data, thread_id(modal.channel_id), &state).await?;

    let msg = build_verdict_message(
        discord_id,
        true,
        &state.players[player_index],
        None,
        all_resolved.then_some(&state),
    );
    send_in_thread(ctx, channel_id.into(), &msg, Vec::new()).await;
    if all_resolved {
        close_thread(ctx, thread_id(modal.channel_id)).await;
    }

    modal
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().content("Rejected"),
        )
        .await?;
    Ok(())
}

async fn finalize_thread_if_resolved(
    ctx: &Context,
    data: &Data,
    thread_id: ThreadId,
    state: &SubmissionState,
) -> Result<bool> {
    if !state
        .players
        .iter()
        .all(|p| p.status != PlayerStatus::Pending)
    {
        return Ok(false);
    }

    cleanup_review_votes(data, thread_id.get());

    let all_approved = state
        .players
        .iter()
        .all(|p| p.status == PlayerStatus::Approved);
    let all_rejected = state
        .players
        .iter()
        .all(|p| p.status == PlayerStatus::Rejected);

    let tags = resolve_forum_tags(ctx, data).await;
    let mut tag_ids = Vec::new();
    if all_approved {
        if let Some(id) = tags.approved {
            tag_ids.push(id);
        }
    } else if all_rejected {
        if let Some(id) = tags.rejected {
            tag_ids.push(id);
        }
    } else if let Some(id) = tags.pending {
        tag_ids.push(id);
    }
    let _ = set_forum_tags(ctx, thread_id, &tag_ids).await;

    Ok(true)
}

async fn close_thread(ctx: &Context, thread_id: ThreadId) {
    let _ = thread_id
        .edit(&ctx.http, EditThread::new().archived(true).locked(true))
        .await;
}

async fn verdict_face(data: &Data, uuid: &str) -> CreateAttachment<'static> {
    let png = data
        .skin_provider
        .fetch_face(uuid, FACE_SIZE)
        .await
        .unwrap_or_else(super::default_face_png);
    CreateAttachment::bytes(png, face_filename(uuid))
}

pub async fn handle_confirm(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let message = component.message.clone();
    let Some(conf) = parse_confirmation_data(&component.data.custom_id, &message) else {
        return send_vote_error(ctx, component, "Could not parse confirmation data").await;
    };

    let submitter_id = component.user.id.get();
    match create_submission(
        ctx,
        data,
        submitter_id,
        &conf.player_name,
        &conf.player_uuid,
        &conf.tag_type,
        &conf.reason,
    )
    .await
    {
        Ok(thread_id) => {
            spawn_submission_timeout(ctx.clone(), thread_id);
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .flags(MessageFlags::IS_COMPONENTS_V2)
                            .components(vec![CreateComponent::Container(CreateContainer::new(vec![text(format!(
                                "## {} Tag Review Created\nYour post is ready in <#{}>.\nAdd evidence there, then submit it for voting.",
                                EMOTE_ADDTAG, thread_id
                            ))]))]),
                    ),
                )
                .await?;
        }
        Err(e) => {
            tracing::error!("Failed to create review submission: {}", e);
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(
                        CreateInteractionResponseMessage::new()
                            .flags(MessageFlags::IS_COMPONENTS_V2)
                            .components(vec![CreateComponent::Container(CreateContainer::new(
                                vec![text("## Error\nFailed to create review submission")],
                            ))]),
                    ),
                )
                .await?;
        }
    }
    Ok(())
}

pub async fn handle_cancel_thread(
    ctx: &Context,
    component: &ComponentInteraction,
    _data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let submitter_id = component.data.custom_id.split(':').last().unwrap_or("0");
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;

    let channel_id: GenericChannelId = component.channel_id.into();
    let delete_msg = CreateMessage::new()
        .content("Deleting post in 30 seconds.")
        .components(vec![CreateComponent::ActionRow(CreateActionRow::Buttons(
            vec![
                CreateButton::new(format!("review_abort_delete:{submitter_id}"))
                    .label("Cancel")
                    .style(ButtonStyle::Secondary),
            ]
            .into(),
        ))]);

    let sent = ctx
        .http
        .send_message(channel_id, Vec::<CreateAttachment>::new(), &delete_msg)
        .await?;
    let http = ctx.http.clone();
    let msg_id = sent.id;

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        let Ok(msg) = http.get_message(channel_id, msg_id).await else {
            return;
        };
        if msg.content != "Deleting post in 30 seconds." {
            return;
        }
        let _ = channel_id.delete(&http, None).await;
    });

    Ok(())
}

pub async fn handle_abort_delete(
    ctx: &Context,
    component: &ComponentInteraction,
    _data: &Data,
) -> Result<()> {
    if !require_submitter(ctx, component).await? {
        return Ok(());
    }

    let channel_id: GenericChannelId = component.channel_id.into();
    let _ = ctx
        .http
        .delete_message(channel_id, component.message.id, None)
        .await;
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await
        .ok();
    Ok(())
}
