use std::collections::HashMap;

use blacklist::{EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE, EMOTE_TAG, lookup as lookup_tag};
use serenity::all::*;

use super::super::channel::{evidence_indicator, format_tag_block, tag_label};
use super::{state::*, *};
use crate::utils::{sanitize_reason, separator, text};

pub const FACE_SIZE: u32 = 128;

pub fn face_filename(uuid: &str) -> String {
    format!("face_{uuid}.png")
}

fn face_url(uuid: &str) -> String {
    format!("attachment://{}", face_filename(uuid))
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

pub struct ReplacedTag {
    pub tag_type: String,
    pub reason: String,
    pub added_line: String,
}

pub fn build_review_message(
    state: &SubmissionState,
    existing_urls: &HashMap<String, String>,
    replaced: &HashMap<String, ReplacedTag>,
) -> Vec<CreateComponent<'static>> {
    let id = state.submitter_id;

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
        parts.push(text(format!("IGN - `{}`", player.username)));
        if let Some(gallery) = media_gallery_for(player, existing_urls) {
            parts.push(gallery);
        }
        if let Some(summary) = render_evidence_summary(player) {
            parts.push(text(summary));
        }

        build_player_card(&mut parts, player, replaced.get(&player.uuid));

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

    let mut components = vec![CreateComponent::Container(CreateContainer::new(parts))];
    if !state.submitted {
        if let Some(idx) = state.editing {
            if idx < state.players.len() {
                components.push(build_edit_manager(state, idx, existing_urls));
            }
        }
    }
    components
}

fn submitter_line(submitter_id: u64, reopened: bool) -> CreateContainerComponent<'static> {
    if reopened {
        text(format!(
            "-# Evidence submitted by <@{submitter_id}> · reopened"
        ))
    } else {
        text(format!("-# Evidence submitted by <@{submitter_id}>"))
    }
}

pub fn build_player_card(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    player: &PlayerEntry,
    replaced: Option<&ReplacedTag>,
) {
    let indicator = evidence_indicator(&player.tag_type, !player.evidence.is_empty());
    let proposed = format_tag_block(
        &player.tag_type,
        &sanitize_reason(&player.reason),
        &indicator,
        None,
        None,
        false,
    );
    let uuid_line = format!(
        "-# UUID: {}",
        crate::utils::format_uuid_dashed(&player.uuid)
    );

    let Some(current) = replaced else {
        parts.push(player_section(
            [proposed, uuid_line].join("\n"),
            &player.uuid,
        ));
        return;
    };

    let current_block = format_tag_block(
        &current.tag_type,
        &sanitize_reason(&current.reason),
        "",
        Some(&current.added_line),
        None,
        false,
    );
    parts.push(text("-# Current"));
    parts.push(player_section(current_block, &player.uuid));
    parts.push(text("-# Proposed"));
    parts.push(text([proposed, uuid_line].join("\n")));
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

fn evidence_media_label(filename: &str) -> String {
    let stem = filename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(filename);
    match stem.rsplit_once('_') {
        Some((name, n)) => format!("{name} ({n})"),
        None => stem.to_string(),
    }
}

fn evidence_piece_label(evidence: &[Evidence], idx: usize) -> String {
    match &evidence[idx] {
        Evidence::Attachment { filename } => evidence_media_label(filename),
        Evidence::Replay { .. } => {
            let n = evidence[..=idx]
                .iter()
                .filter(|e| matches!(e, Evidence::Replay { .. }))
                .count();
            format!("Replay {n}")
        }
    }
}

pub fn build_edit_manager(
    state: &SubmissionState,
    idx: usize,
    existing_urls: &HashMap<String, String>,
) -> CreateComponent<'static> {
    let id = state.submitter_id;
    let player = &state.players[idx];

    let mut parts: Vec<CreateContainerComponent> =
        vec![text(format!("## {EMOTE_TAG} Edit — `{}`", player.username))];

    if !player.evidence.is_empty() {
        let sel = state.editing_evidence.min(player.evidence.len() - 1);
        parts.push(separator());
        parts.push(text(format!(
            "**{}**",
            evidence_piece_label(&player.evidence, sel)
        )));
        match &player.evidence[sel] {
            Evidence::Attachment { filename } => {
                let url = existing_urls
                    .get(filename)
                    .cloned()
                    .unwrap_or_else(|| format!("attachment://{filename}"));
                parts.push(CreateContainerComponent::MediaGallery(
                    CreateMediaGallery::new(vec![CreateMediaGalleryItem::new(
                        CreateUnfurledMediaItem::new(url),
                    )]),
                ));
            }
            Evidence::Replay { replay, note } => {
                parts.push(text(render_replay_line(replay, note.as_deref())));
            }
        }

        if player.evidence.len() > 1 {
            let options: Vec<CreateSelectMenuOption<'static>> = player
                .evidence
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != sel)
                .map(|(i, _)| {
                    CreateSelectMenuOption::new(
                        evidence_piece_label(&player.evidence, i),
                        i.to_string(),
                    )
                })
                .collect();
            parts.push(CreateContainerComponent::ActionRow(
                CreateActionRow::SelectMenu(
                    CreateSelectMenu::new(
                        format!("review_evsel:{idx}:{id}"),
                        CreateSelectMenuKind::String {
                            options: options.into(),
                        },
                    )
                    .placeholder("View another piece..."),
                ),
            ));
        }

        let mut piece_buttons = Vec::new();
        if matches!(&player.evidence[sel], Evidence::Replay { .. }) {
            piece_buttons.push(
                CreateButton::new(format!("review_edit_replay:{idx}:{sel}:{id}"))
                    .label("Edit Replay")
                    .style(ButtonStyle::Secondary),
            );
        }
        piece_buttons.push(
            CreateButton::new(format!("review_remove_evidence:{idx}:{sel}:{id}"))
                .label("Remove Evidence")
                .style(ButtonStyle::Danger),
        );
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(piece_buttons.into()),
        ));
    }

    parts.push(separator());
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

    CreateComponent::Container(CreateContainer::new(parts))
}

pub fn build_submitted_controls(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    player: &PlayerEntry,
    idx: usize,
    id: u64,
) {
    if player.status == PlayerStatus::Pending {
        let disagreement = has_disagreement(player);
        let accepts = player.accept_votes.len();
        let rejects = player.reject_votes.len();
        parts.push(text(vote_tally(
            EMOTE_EVIDENCE,
            "Accept",
            &player.accept_votes,
            super::ACCEPT_THRESHOLD,
            disagreement,
            rejects,
        )));
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(
                vec![
                    CreateButton::new(format!("review_approve:{idx}:{id}"))
                        .label("Accept")
                        .style(ButtonStyle::Success),
                ]
                .into(),
            ),
        ));
        parts.push(text(vote_tally(
            EMOTE_NO_EVIDENCE,
            "Reject",
            &player.reject_votes,
            super::REJECT_THRESHOLD,
            disagreement,
            accepts,
        )));
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(
                vec![
                    CreateButton::new(format!("review_reject:{idx}:{id}"))
                        .label("Reject")
                        .style(ButtonStyle::Danger),
                ]
                .into(),
            ),
        ));
        if disagreement {
            parts.push(text("-# Votes disagree — a moderator must resolve this"));
        }
    } else {
        parts.push(text(render_status_line(player)));
    }
}

fn vote_tally(
    emote: &str,
    label: &str,
    votes: &[u64],
    threshold: usize,
    disagreement: bool,
    other_count: usize,
) -> String {
    let count = votes.len();
    if disagreement {
        format!("### {emote} {label} — {}", mentions(votes))
    } else if count > 0 {
        format!(
            "### {emote} {label} [{count}/{threshold}] — {}",
            mentions(votes)
        )
    } else if other_count == 0 {
        format!("### {emote} {label} [0/{threshold}]")
    } else {
        format!("### {emote} {label}")
    }
}

fn mentions(ids: &[u64]) -> String {
    ids.iter()
        .map(|id| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join(", ")
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
    if state.reopened {
        buttons.push(
            CreateButton::new(format!("review_submit:{id}"))
                .label("Submit")
                .style(ButtonStyle::Success),
        );
        buttons.push(
            CreateButton::new(format!("review_cancel_thread:{id}"))
                .label("Cancel Review")
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
                    .label("Cancel Review")
                    .style(ButtonStyle::Danger),
            ]
            .into(),
        )),
    ]))]
}

pub fn build_vote_components(
    voter_id: u64,
    vote_type: &str,
    tag_type: &str,
    username: &str,
    changed: bool,
) -> Vec<CreateComponent<'static>> {
    let label = tag_label(tag_type);
    let content = if changed {
        format!(
            "<@{voter_id}> changed their vote to **{vote_type}** the {label} tag on `{username}`."
        )
    } else {
        format!("<@{voter_id}> voted to **{vote_type}** the {label} tag on `{username}`.")
    };
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(content),
    ]))]
}

pub fn build_verdict_message(
    voter_id: u64,
    is_staff: bool,
    decided: &PlayerEntry,
    stored_tag_type: Option<&str>,
    submitter_summary: Option<&SubmissionState>,
) -> CreateMessage<'static> {
    let approved = decided.status == PlayerStatus::Approved;
    let label = tag_label(&decided.tag_type);

    let action = if approved { "approved" } else { "rejected" };
    let announce = if is_staff {
        format!(
            "<@{voter_id}> **{action}** the {label} tag on `{}`.",
            decided.username
        )
    } else {
        format!(
            "The {label} tag on `{}` was **{action}** by review.",
            decided.username
        )
    };

    let mut content = announce;

    if let Some(state) = submitter_summary {
        content.push_str(&format!(
            "\n\n<@{}> All players have been reviewed:",
            state.submitter_id
        ));
        for p in &state.players {
            let p_emote = lookup_tag(&p.tag_type).map(|d| d.emote).unwrap_or("");
            let v = match p.status {
                PlayerStatus::Approved => "approved",
                PlayerStatus::Rejected => "rejected",
                PlayerStatus::Pending => "pending",
            };
            content.push_str(&format!("\n- {p_emote} `{}` — **{v}**", p.username));
        }
    }

    let mut components = vec![CreateComponent::Container(CreateContainer::new(vec![
        text(content),
    ]))];

    if approved {
        if let Some(stored) = stored_tag_type {
            components.push(CreateComponent::Container(build_tag_preview(
                &decided.username,
                &decided.uuid,
                stored,
                &decided.reason,
            )));
        }
    }

    CreateMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components)
}

fn build_tag_preview(
    username: &str,
    uuid: &str,
    stored_tag_type: &str,
    reason: &str,
) -> CreateContainer<'static> {
    let header = format!(
        "## {} New Tag\nIGN - `{username}`\n",
        blacklist::EMOTE_ADDTAG
    );
    let block = format_tag_block(
        stored_tag_type,
        &sanitize_reason(reason),
        "",
        None,
        None,
        false,
    );
    let footer = format!("-# UUID: {}", crate::utils::format_uuid_dashed(uuid));

    CreateContainer::new(vec![
        player_section(format!("{header}{block}\n{footer}"), uuid),
        separator(),
    ])
}

pub struct CurrentTag {
    pub tag_type: String,
    pub detail: String,
    pub added_line: String,
}

pub fn build_confirmation_message(
    submitter_id: u64,
    player_name: &str,
    player_uuid: &str,
    tag_type: &str,
    reason: &str,
    forum_id: Option<ChannelId>,
    current: Option<CurrentTag>,
) -> Vec<CreateComponent<'static>> {
    let confirm_id = format!("review_confirm:{submitter_id}:{tag_type}:{player_uuid}");
    let destination = match forum_id {
        Some(id) => format!("<#{id}>"),
        None => "the review channel".to_string(),
    };

    let proposed = format_tag_block(tag_type, &sanitize_reason(reason), "", None, None, false);
    let create_button = CreateButton::new(confirm_id)
        .label("Create Post")
        .style(ButtonStyle::Success);

    let mut parts: Vec<CreateContainerComponent> = vec![text(format!("## {EMOTE_TAG} Tag Review"))];

    if let Some(current) = current {
        let current_block = format_tag_block(
            &current.tag_type,
            &current.detail,
            "",
            Some(&current.added_line),
            None,
            false,
        );
        parts.push(text(format!(
            "This player already has an incompatible tag. Confirming opens a post in {destination} where you add evidence and others vote — if approved, your tag replaces the current one."
        )));
        parts.push(separator());
        parts.push(text("-# Current"));
        parts.push(player_section(
            [format!("IGN - `{player_name}`\n"), current_block].join("\n"),
            player_uuid,
        ));
        parts.push(separator());
        parts.push(text("-# Proposed"));
        parts.push(CreateContainerComponent::Section(CreateSection::new(
            vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                proposed,
            ))],
            CreateSectionAccessory::Button(create_button),
        )));
    } else {
        parts.push(text(format!(
            "This tag needs approval first. Confirming opens a post in {destination} where you'll add evidence, then others vote on it."
        )));
        parts.push(separator());
        parts.push(text("-# Preview"));
        parts.push(player_section(
            [format!("IGN - `{player_name}`\n"), proposed].join("\n"),
            player_uuid,
        ));
        parts.push(separator());
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::Buttons(vec![create_button].into()),
        ));
    }

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}
