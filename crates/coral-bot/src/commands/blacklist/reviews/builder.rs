use std::collections::HashMap;

use blacklist::{EMOTE_TAG, lookup as lookup_tag};
use serenity::all::*;

use super::{state::*, *};
use crate::utils::{sanitize_reason, separator, text};

const STEVE_UUID: &str = "8667ba71b85a4004af54457a9734eed7";

pub fn face_url(uuid: &str) -> String {
    let id = if uuid.is_empty() { STEVE_UUID } else { uuid };
    format!("https://mc-heads.net/avatar/{id}/128")
}

fn player_section(content: String, uuid: &str) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            content,
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            face_url(uuid),
        ))),
    ))
}

pub fn build_review_message(
    state: &SubmissionState,
    existing_urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    let id = state.submitter_id;

    if !state.submitted {
        if let Some(idx) = state.editing {
            if idx < state.players.len() {
                return build_edit_page(state, idx, existing_urls);
            }
        }
    }

    let player_count = state.players.len();
    let header = if player_count > 1 {
        format!("## {EMOTE_TAG} Tag Review · {player_count} players")
    } else {
        format!("## {EMOTE_TAG} Tag Review")
    };
    let mut parts: Vec<CreateContainerComponent> = vec![text(header), separator()];

    if state.players.is_empty() && state.pending_add.is_none() {
        parts.push(text("-# No players added yet"));
        parts.push(separator());
    }

    for (idx, player) in state.players.iter().enumerate() {
        if let Some(gallery) = media_gallery_for(player, existing_urls) {
            parts.push(gallery);
        }
        if let Some(summary) = render_evidence_summary(player) {
            parts.push(text(summary));
        }

        build_player_card(&mut parts, player);

        if state.submitted {
            build_submitted_controls(&mut parts, player, idx, id);
        } else {
            build_evidence_controls(&mut parts, idx, id);
        }

        parts.push(separator());
    }

    if let Some(pending) = &state.pending_add {
        build_pending_add_section(&mut parts, pending, id);
        parts.push(separator());
    }

    if state.submitted {
        build_submitted_footer(&mut parts, state, id);
    } else {
        build_editing_footer(&mut parts, state, id);
    }

    parts.push(submitter_line(state.submitter_id, state.reopened));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn submitter_line(submitter_id: u64, reopened: bool) -> CreateContainerComponent<'static> {
    if reopened {
        text(format!("-# Submitted by <@{submitter_id}> · reopened"))
    } else {
        text(format!("-# Submitted by <@{submitter_id}>"))
    }
}

pub fn build_player_card(parts: &mut Vec<CreateContainerComponent<'static>>, player: &PlayerEntry) {
    let def = lookup_tag(&player.tag_type);
    let emote = def.map(|d| d.emote).unwrap_or("");
    let display_name = def.map(|d| d.display_name).unwrap_or(&player.tag_type);

    let mut lines = vec![
        format!("IGN - `{}`", player.username),
        format!("**{emote} {display_name}**"),
        format!("> {}", sanitize_reason(&player.reason)),
    ];
    if let Some(warning) = &player.conflict_warning {
        lines.push(warning.clone());
    }
    lines.push(format!(
        "-# UUID: {}",
        crate::utils::format_uuid_dashed(&player.uuid)
    ));

    parts.push(player_section(lines.join("\n"), &player.uuid));
}

pub fn build_evidence_controls(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    idx: usize,
    id: u64,
) {
    let buttons = vec![
        CreateButton::new(format!("review_add_replay:{idx}:{id}"))
            .label("+ Replay")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("review_attach_media:{idx}:{id}"))
            .label("+ Media")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("review_edit_tag:{idx}:{id}"))
            .label("Edit")
            .style(ButtonStyle::Secondary),
    ];
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(buttons.into()),
    ));
}

pub fn build_edit_page(
    state: &SubmissionState,
    idx: usize,
    existing_urls: &HashMap<String, String>,
) -> Vec<CreateComponent<'static>> {
    let id = state.submitter_id;
    let player = &state.players[idx];

    let mut parts: Vec<CreateContainerComponent> =
        vec![text(format!("## {EMOTE_TAG} Edit Tag")), separator()];

    for (ev_idx, ev) in player.evidence.iter().enumerate() {
        let remove = CreateButton::new(format!("review_remove_evidence:{idx}:{ev_idx}:{id}"))
            .label("Remove")
            .style(ButtonStyle::Danger);
        match ev {
            Evidence::Replay { replay, note } => {
                parts.push(CreateContainerComponent::Section(CreateSection::new(
                    vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                        render_replay_line(replay, note.as_deref()),
                    ))],
                    CreateSectionAccessory::Button(remove),
                )));
            }
            Evidence::Attachment { filename } => {
                let url = existing_urls
                    .get(filename)
                    .cloned()
                    .unwrap_or_else(|| format!("attachment://{filename}"));
                parts.push(CreateContainerComponent::Section(CreateSection::new(
                    vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                        format!("`{filename}`"),
                    ))],
                    CreateSectionAccessory::Thumbnail(CreateThumbnail::new(
                        CreateUnfurledMediaItem::new(url),
                    )),
                )));
                parts.push(CreateContainerComponent::ActionRow(
                    CreateActionRow::Buttons(vec![remove].into()),
                ));
            }
        }
    }

    build_player_card(&mut parts, player);

    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("review_tag_select_edit:{idx}:{id}"),
                CreateSelectMenuKind::String {
                    options: build_tag_select_options(Some(&player.tag_type)).into(),
                },
            )
            .placeholder("Change tag type"),
        ),
    ));

    let mut controls = vec![
        CreateButton::new(format!("review_edit_reason:{idx}:{id}"))
            .label("Edit Reason")
            .style(ButtonStyle::Secondary),
    ];
    if state.players.len() > 1 {
        controls.push(
            CreateButton::new(format!("review_remove_player:{idx}:{id}"))
                .label("Remove Tag")
                .style(ButtonStyle::Danger),
        );
    }
    controls.push(
        CreateButton::new(format!("review_edit_done:{idx}:{id}"))
            .label("Done")
            .style(ButtonStyle::Primary),
    );
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(controls.into()),
    ));

    parts.push(submitter_line(id, state.reopened));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

pub fn build_submitted_controls(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    player: &PlayerEntry,
    idx: usize,
    id: u64,
) {
    if player.status == PlayerStatus::Pending {
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(
                vec![
                    CreateButton::new(format!("review_approve:{idx}:{id}"))
                        .label("Accept")
                        .style(ButtonStyle::Success),
                    CreateButton::new(format!("review_reject:{idx}:{id}"))
                        .label("Reject")
                        .style(ButtonStyle::Danger),
                ]
                .into(),
            ),
        ));
        parts.push(text(render_vote_indicator(player)));
        if has_disagreement(player) {
            parts.push(text("-# Mod needed to resolve"));
        }
    } else {
        parts.push(text(render_status_line(player)));
    }
}

pub fn render_vote_indicator(player: &PlayerEntry) -> String {
    let accepts = player.accept_votes.len();
    let rejects = player.reject_votes.len();
    let threshold = VOTE_THRESHOLD;

    if accepts > 0 && rejects > 0 {
        return format!("✅ {accepts} · ❌ {rejects}");
    }

    let (emoji, count) = if accepts > 0 {
        ("✅ ", accepts)
    } else if rejects > 0 {
        ("❌ ", rejects)
    } else {
        ("", 0)
    };

    let filled = count.min(threshold);
    let empty = threshold.saturating_sub(filled);
    let boxes: String = "■".repeat(filled) + &"□".repeat(empty);
    format!("{emoji}[{boxes}] {count}/{threshold}")
}

pub fn has_disagreement(player: &PlayerEntry) -> bool {
    !player.accept_votes.is_empty() && !player.reject_votes.is_empty()
}

pub fn build_pending_add_section(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    pending: &PendingAdd,
    id: u64,
) {
    parts.push(text(format!(
        "Adding **`{}`** \u{2014} select a tag type:",
        pending.username
    )));

    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("review_pending_tag:{}:{}", pending.identifier, id),
                CreateSelectMenuKind::String {
                    options: build_tag_select_options(None).into(),
                },
            )
            .placeholder("Select tag type"),
        ),
    ));
}

pub fn build_submitted_footer(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    state: &SubmissionState,
    id: u64,
) {
    if state
        .players
        .iter()
        .any(|p| p.status == PlayerStatus::Pending)
    {
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(
                vec![
                    CreateButton::new(format!("review_edit_submitted:{id}"))
                        .label("Edit")
                        .style(ButtonStyle::Secondary),
                ]
                .into(),
            ),
        ));
    }
}

pub fn build_editing_footer(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    state: &SubmissionState,
    id: u64,
) {
    let mut buttons = Vec::new();
    if state.players.len() < 4 && state.pending_add.is_none() {
        buttons.push(
            CreateButton::new(format!("review_add_player:{id}"))
                .label("+ Player")
                .style(ButtonStyle::Primary),
        );
    }
    // After an initial submission, Submit/Cancel live on the OP itself so they
    // stay near the post instead of below the thread's discussion.
    if state.reopened {
        buttons.push(
            CreateButton::new(format!("review_submit:{id}"))
                .label("Submit")
                .style(ButtonStyle::Success),
        );
        buttons.push(
            CreateButton::new(format!("review_cancel_thread:{id}"))
                .label("Cancel")
                .style(ButtonStyle::Danger),
        );
    }
    if !buttons.is_empty() {
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(buttons.into()),
        ));
    }
}

pub fn build_submit_reminder(submitter_id: u64) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(
            "Add your evidence to the post above, then press **Submit** when you're ready for review.",
        ),
        CreateContainerComponent::ActionRow(CreateActionRow::Buttons(
            vec![
                CreateButton::new(format!("review_submit:{submitter_id}"))
                    .label("Submit")
                    .style(ButtonStyle::Success),
                CreateButton::new(format!("review_cancel_thread:{submitter_id}"))
                    .label("Cancel")
                    .style(ButtonStyle::Danger),
            ]
            .into(),
        )),
    ]))]
}

pub fn build_vote_message(
    voter_id: u64,
    vote_type: &str,
    tag_type: &str,
    username: &str,
    is_staff: bool,
    accept_count: usize,
    reject_count: usize,
) -> CreateMessage<'static> {
    let def = lookup_tag(tag_type);
    let emote = def.map(|d| d.emote).unwrap_or("");
    let display_name = def.map(|d| d.display_name).unwrap_or(tag_type);

    let content = if is_staff {
        let action = if vote_type == "accept" {
            "approved"
        } else {
            "rejected"
        };
        format!("<@{voter_id}> {action} the {emote} **{display_name}** tag on `{username}`.")
    } else {
        let total = if vote_type == "accept" {
            accept_count
        } else {
            reject_count
        };
        let mut msg = format!(
            "<@{voter_id}> voted to **{vote_type}** the {emote} **{display_name}** tag on `{username}`. [{total}/3]"
        );
        if accept_count > 0 && reject_count > 0 {
            msg.push_str(&format!(
                "\n-# {accept_count} accept, {reject_count} reject \u{2014} staff required to resolve"
            ));
        }
        msg
    };

    CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(vec![CreateComponent::Container(CreateContainer::new(
            vec![text(content)],
        ))])
}

pub fn build_confirmation_message(
    submitter_id: u64,
    player_name: &str,
    player_uuid: &str,
    tag_type: &str,
    reason: &str,
    forum_id: Option<ChannelId>,
) -> Vec<CreateComponent<'static>> {
    let def = lookup_tag(tag_type);
    let emote = def.map(|d| d.emote).unwrap_or("");
    let display_name = def.map(|d| d.display_name).unwrap_or(tag_type);

    let confirm_id = format!("review_confirm:{submitter_id}:{tag_type}:{player_uuid}");

    let destination = match forum_id {
        Some(id) => format!("<#{id}>"),
        None => "the review channel".to_string(),
    };

    let preview = player_section(
        format!(
            "IGN - `{player_name}`\n{emote} **{display_name}**\n> {}",
            sanitize_reason(reason)
        ),
        player_uuid,
    );

    let mut parts: Vec<CreateContainerComponent> = vec![
        text(format!("## {EMOTE_TAG} Create Tag Review Post")),
        text(format!(
            "This tag needs approval first. Confirming opens a post in {destination} where you'll add evidence, then others vote on it."
        )),
        separator(),
        text("-# Preview"),
        preview,
    ];

    parts.push(separator());
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(
            vec![
                CreateButton::new(confirm_id)
                    .label("Create Post")
                    .style(ButtonStyle::Success),
                CreateButton::new(format!("review_cancel:{submitter_id}"))
                    .label("Cancel")
                    .style(ButtonStyle::Secondary),
            ]
            .into(),
        ),
    ));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}
