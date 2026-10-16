use std::collections::HashMap;

use blacklist::{
    EMOTE_EDITTAG, EMOTE_EVIDENCE, EMOTE_NO_EVIDENCE, EMOTE_TAG, Replay, lookup as lookup_tag,
};
use chrono::{DateTime, Utc};
use serenity::all::*;

use database::{
    ConfirmState, Evidence, EvidenceKind, PromptTally, ReviewRules, ReviewStatus, RevisionRequest,
    Tally, Vote, VoteKind,
};

use super::super::channel::{format_ign, format_tag_block};
use super::community::{ADD, DISMISS};
use super::{
    EVIDENCE_VOTES_NEEDED, Loaded, REVIEW_TAGS, REVISION_VOTES_NEEDED, custom_id, custom_id_with,
    face_filename, media_label,
};
use crate::utils::{format_uuid_dashed, sanitize_reason, separator, text};

pub const ANONYMITY_NOTE: &str =
    "Be aware that users may still be able to determine who made this report with the replay.";
const MAX_LISTED_REQUESTS: usize = 4;
const EVIDENCE_QUESTION: &str = "Add this to the review's evidence?";

pub enum ConsoleMode {
    Draft {
        expires_at: DateTime<Utc>,
    },
    Edit {
        save_clears_votes: bool,
        can_withdraw: bool,
    },
}

pub struct MediaUrls {
    pub evidence: HashMap<i64, String>,
    pub face: Option<String>,
}

impl MediaUrls {
    pub fn attached() -> Self {
        Self {
            evidence: HashMap::new(),
            face: None,
        }
    }
}

pub fn console(
    loaded: &Loaded,
    mode: &ConsoleMode,
    notice: Option<&str>,
) -> Vec<CreateComponent<'static>> {
    let review_id = loaded.review.id;
    let has_replay = loaded
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::Replay);
    let mut parts = vec![
        text(format!("## {EMOTE_TAG} Tag Review")),
        separator(),
        text(format!("IGN - `{}`", loaded.username)),
    ];
    if let Some(list) = evidence_list(&loaded.evidence) {
        parts.push(text(list));
    }
    parts.push(tag_card(
        loaded,
        &format!("attachment://{}", face_filename(&loaded.review.player_uuid)),
    ));
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(custom_id("replay", review_id))
                .label("+ Replay")
                .style(ButtonStyle::Primary),
            CreateButton::new(custom_id("media", review_id))
                .label("+ Media")
                .style(ButtonStyle::Primary),
            CreateButton::new(custom_id("manage", review_id))
                .label("Edit")
                .style(ButtonStyle::Secondary),
        ]),
    ));
    parts.push(separator());

    if let Some(notice) = notice {
        parts.push(text(format!("-# {notice}")));
    }
    parts.push(text(match (mode, has_replay) {
        (_, false) => "Add a replay to submit this for review.".to_string(),
        (ConsoleMode::Draft { .. }, true) => {
            "Add your evidence, then press **Submit** when you're ready for review.".to_string()
        }
        (ConsoleMode::Edit { .. }, true) => {
            "Make your changes, then press **Save** to update the review.".to_string()
        }
    }));
    if loaded.review.hide_username {
        parts.push(text(format!("-# {ANONYMITY_NOTE}")));
    }
    parts.extend(console_actions(review_id, mode, has_replay));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn console_actions(
    review_id: i64,
    mode: &ConsoleMode,
    has_replay: bool,
) -> Vec<CreateContainerComponent<'static>> {
    match mode {
        ConsoleMode::Draft { expires_at } => vec![
            CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                CreateButton::new(custom_id("submit", review_id))
                    .label("Submit")
                    .style(ButtonStyle::Success)
                    .disabled(!has_replay),
                CreateButton::new(custom_id("discard", review_id))
                    .label("Discard")
                    .style(ButtonStyle::Danger),
            ])),
            text(format!(
                "-# This draft expires <t:{}:R> if left untouched.",
                expires_at.timestamp()
            )),
        ],
        ConsoleMode::Edit {
            save_clears_votes,
            can_withdraw,
            ..
        } => {
            let mut parts = Vec::new();
            if *save_clears_votes {
                parts.push(text(
                    "-# Saving will clear accept votes so that reviewers read what changed.",
                ));
            }
            parts.push(CreateContainerComponent::ActionRow(
                CreateActionRow::buttons(vec![
                    CreateButton::new(custom_id("save", review_id))
                        .label("Save")
                        .style(ButtonStyle::Success)
                        .disabled(!has_replay),
                    CreateButton::new(custom_id("discard", review_id))
                        .label("Cancel edit")
                        .style(ButtonStyle::Secondary),
                ]),
            ));
            if *can_withdraw {
                parts.push(CreateContainerComponent::ActionRow(
                    CreateActionRow::buttons(vec![
                        CreateButton::new(custom_id("withdraw", review_id))
                            .label("Withdraw review")
                            .style(ButtonStyle::Danger),
                    ]),
                ));
            } else {
                parts.push(text(
                    "-# You can't withdraw a review once anyone has voted on it. If you made a \
                     mistake, ask a staff member.",
                ));
            }
            parts
        }
    }
}

pub fn evidence_manager(loaded: &Loaded, selected: Option<i64>) -> Vec<CreateComponent<'static>> {
    let review_id = loaded.review.id;
    let mut parts = vec![text(format!("## {EMOTE_TAG} Edit `{}`", loaded.username))];
    let current = selected
        .and_then(|id| loaded.evidence.iter().find(|e| e.id == id))
        .or_else(|| loaded.evidence.first());

    if let Some(piece) = current {
        parts.push(separator());
        parts.push(text(format!(
            "**{}**\n{}",
            piece_label(&loaded.evidence, piece),
            piece_line(piece)
        )));
        if loaded.evidence.len() > 1 {
            let options: Vec<CreateSelectMenuOption> = loaded
                .evidence
                .iter()
                .filter(|e| e.id != piece.id)
                .map(|e| {
                    CreateSelectMenuOption::new(piece_label(&loaded.evidence, e), e.id.to_string())
                })
                .collect();
            parts.push(CreateContainerComponent::ActionRow(
                CreateActionRow::SelectMenu(
                    CreateSelectMenu::new(
                        custom_id("evsel", review_id),
                        CreateSelectMenuKind::String {
                            options: options.into(),
                        },
                    )
                    .placeholder("View another piece..."),
                ),
            ));
        }
        let edit_button = match piece.kind {
            EvidenceKind::Replay => {
                CreateButton::new(custom_id_with("evreplay", review_id, piece.id))
                    .label("Edit Replay")
            }
            EvidenceKind::Media => {
                CreateButton::new(custom_id_with("evnote", review_id, piece.id)).label("Edit Note")
            }
        };
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::buttons(vec![
                edit_button.style(ButtonStyle::Secondary),
                CreateButton::new(custom_id_with("evdel", review_id, piece.id))
                    .label("Remove Evidence")
                    .style(ButtonStyle::Danger),
            ]),
        ));
    }

    parts.push(separator());
    let type_options: Vec<CreateSelectMenuOption> = blacklist::all()
        .iter()
        .filter(|def| REVIEW_TAGS.contains(&def.name))
        .map(|def| {
            CreateSelectMenuOption::new(def.display_name, def.name)
                .default_selection(def.name == loaded.revision.tag_type)
        })
        .collect();
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                custom_id("type", review_id),
                CreateSelectMenuKind::String {
                    options: type_options.into(),
                },
            )
            .placeholder("Change tag type"),
        ),
    ));
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(custom_id("reason", review_id))
                .label("Edit Reason")
                .style(ButtonStyle::Secondary),
            CreateButton::new(custom_id("done", review_id))
                .label("Done")
                .style(ButtonStyle::Primary),
        ]),
    ));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

pub struct PostState<'a> {
    pub tally: Tally,
    pub revise_supporters: i64,
    pub votes: &'a [Vote],
    pub paused: bool,
    pub edited_at: Option<DateTime<Utc>>,
    pub rules: ReviewRules,
}

pub fn public_post(
    loaded: &Loaded,
    state: &PostState,
    urls: &MediaUrls,
) -> Vec<CreateComponent<'static>> {
    let review = &loaded.review;
    let face = urls
        .face
        .clone()
        .unwrap_or_else(|| format!("attachment://{}", face_filename(&review.player_uuid)));
    let mut parts = vec![
        text(format!("## {EMOTE_TAG} Tag Review")),
        separator(),
        text(format_ign(&loaded.username, review.player)),
    ];
    if let Some(gallery) = media_gallery(&loaded.evidence, urls) {
        parts.push(gallery);
    }
    if let Some(replays) = replay_lines(&loaded.evidence) {
        parts.push(text(replays));
    }
    parts.push(tag_card(loaded, &face));
    if let Some(at) = state.edited_at {
        parts.push(text(format!("-# Edited <t:{}:R>", at.timestamp())));
    }
    parts.push(separator());

    if review.status == ReviewStatus::Open {
        parts.extend(vote_sections(review.id, state));
    } else {
        parts.push(text(outcome_text(loaded, state.votes)));
    }
    if review.confirm == Some(ConfirmState::Pending) {
        parts.push(separator());
        parts.extend(confirm_section(review.id));
    }

    parts.push(separator());
    let submitted = if review.hide_username {
        "-# Tag submitted anonymously".to_string()
    } else {
        format!("-# Tag submitted by <@{}>", review.submitter_id)
    };
    parts.push(text(match review.confirm.and_then(confirm_line) {
        Some(line) => format!("{submitted}\n{line}"),
        None => submitted,
    }));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn confirm_section(review_id: i64) -> Vec<CreateContainerComponent<'static>> {
    let emote = lookup_tag("confirmed_cheater").map_or("", |d| d.emote);
    vec![
        text(format!("### {emote} Confirm Tag")),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
            CreateButton::new(custom_id("confirmtag", review_id))
                .label("Confirm Tag")
                .style(ButtonStyle::Success),
            CreateButton::new(custom_id("dismisstag", review_id))
                .label("Dismiss")
                .style(ButtonStyle::Secondary),
        ])),
    ]
}

fn confirm_line(state: ConfirmState) -> Option<String> {
    match state {
        ConfirmState::Pending => None,
        ConfirmState::Confirmed(id) => Some(format!("-# Confirmed by <@{id}>")),
        ConfirmState::Dismissed(id) => Some(format!("-# Dismissed by <@{id}>")),
        ConfirmState::Closed(id) => Some(format!("-# Closed by <@{id}>")),
    }
}

fn vote_sections(review_id: i64, state: &PostState) -> Vec<CreateContainerComponent<'static>> {
    let tally = state.tally;
    let contested = tally.accept > 0 && tally.reject > 0;
    let revise_emote = lookup_tag("replays_needed").map_or("", |d| d.emote);
    let accept_heading = if contested {
        format!("### {EMOTE_EVIDENCE} Accept [{}]", tally.accept)
    } else {
        format!(
            "### {EMOTE_EVIDENCE} Accept [{}/{}]",
            tally.accept, state.rules.accept_threshold
        )
    };
    let reject_heading = if contested {
        format!("### {EMOTE_NO_EVIDENCE} Reject [{}]", tally.reject)
    } else {
        format!(
            "### {EMOTE_NO_EVIDENCE} Reject [{}/{}]",
            tally.reject, state.rules.reject_threshold
        )
    };
    let revise_heading = format!("### {revise_emote} Revise [{}]", state.revise_supporters);

    let mut parts = vec![
        vote_section(
            accept_heading,
            custom_id("accept", review_id),
            "Accept",
            ButtonStyle::Success,
            state.paused,
        ),
        vote_section(
            reject_heading,
            custom_id("reject", review_id),
            "Reject",
            ButtonStyle::Danger,
            state.paused,
        ),
        vote_section(
            revise_heading,
            custom_id("revise", review_id),
            "Revise",
            ButtonStyle::Secondary,
            state.paused,
        ),
    ];
    let status = if state.paused {
        Some("Voting is paused while the submitter is editing.")
    } else if contested {
        Some("Votes disagree, so a staff member must resolve this.")
    } else {
        None
    };
    if let Some(status) = status {
        parts.push(text(format!("-# {status}")));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(custom_id("editpost", review_id))
                .label("Edit")
                .style(ButtonStyle::Secondary),
        ]),
    ));
    parts
}

pub fn revision_request(
    review_id: i64,
    request: &RevisionRequest,
    votable: bool,
) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![text(format!(
        "## {EMOTE_EDITTAG} Revision\n{}",
        request_summary(request)
    ))];
    if votable {
        parts.push(separator());
        parts.extend(request_votes(review_id, request));
    }
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

pub fn applied_revision(
    request: &RevisionRequest,
    by_submitter: bool,
) -> Vec<CreateComponent<'static>> {
    let applied_by = if by_submitter {
        "the submitter"
    } else {
        "vote"
    };
    simple(format!(
        "## {EMOTE_EDITTAG} Revision\n{}\n-# Applied by {applied_by}.",
        request_summary(request)
    ))
}

pub fn revise_menu(review_id: i64, requests: &[RevisionRequest]) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![text(format!("## {EMOTE_EDITTAG} Revise"))];
    let skipped = requests.len().saturating_sub(MAX_LISTED_REQUESTS);
    for request in &requests[skipped..] {
        parts.push(separator());
        parts.push(text(request_summary(request)));
        parts.extend(request_votes(review_id, request));
    }
    parts.push(separator());
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(custom_id("rqnew", review_id))
                .label("New Revision")
                .style(ButtonStyle::Primary),
        ]),
    ));
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn request_summary(request: &RevisionRequest) -> String {
    format_tag_block(
        &request.tag_type,
        &sanitize_reason(&request.reason),
        "",
        None,
        None,
        false,
    )
}

fn request_votes(
    review_id: i64,
    request: &RevisionRequest,
) -> Vec<CreateContainerComponent<'static>> {
    vec![
        vote_section(
            format!("### Yes [{}/{REVISION_VOTES_NEEDED}]", request.tally.yes),
            custom_id_with("rqyes", review_id, request.id),
            "Yes",
            ButtonStyle::Success,
            false,
        ),
        vote_section(
            format!("### No [{}/{REVISION_VOTES_NEEDED}]", request.tally.no),
            custom_id_with("rqno", review_id, request.id),
            "No",
            ButtonStyle::Danger,
            false,
        ),
    ]
}

pub fn evidence_prompt(
    review_id: i64,
    source_id: u64,
    tally: PromptTally,
    votable: bool,
) -> Vec<CreateComponent<'static>> {
    let mut parts = vec![text(EVIDENCE_QUESTION)];
    if votable {
        parts.push(separator());
        parts.push(vote_section(
            format!("### Add [{}/{EVIDENCE_VOTES_NEEDED}]", tally.add),
            custom_id_with(ADD, review_id, source_id),
            "Add",
            ButtonStyle::Success,
            false,
        ));
        parts.push(vote_section(
            format!("### Dismiss [{}/{EVIDENCE_VOTES_NEEDED}]", tally.dismiss),
            custom_id_with(DISMISS, review_id, source_id),
            "Dismiss",
            ButtonStyle::Danger,
            false,
        ));
    }
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn vote_section(
    heading: String,
    id: String,
    label: &'static str,
    style: ButtonStyle,
    paused: bool,
) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            heading,
        ))],
        CreateSectionAccessory::Button(
            CreateButton::new(id)
                .label(label)
                .style(style)
                .disabled(paused),
        ),
    ))
}

fn outcome_text(loaded: &Loaded, votes: &[Vote]) -> String {
    let review = &loaded.review;
    let voters = |kind: VoteKind| -> Vec<String> {
        votes
            .iter()
            .filter(|v| v.kind == kind)
            .map(|v| format!("<@{}>", v.voter_id))
            .collect()
    };
    let resolved_by = review
        .staff_resolved
        .then_some(review.resolved_by)
        .flatten()
        .map(|id| format!("Resolved by <@{id}>."));
    match review.status {
        ReviewStatus::Accepted => format!(
            "### {EMOTE_EVIDENCE} Approved\n-# {}",
            outcome_line(resolved_by, "Accepted by", voters(VoteKind::Accept))
        ),
        ReviewStatus::Rejected => format!(
            "### {EMOTE_NO_EVIDENCE} Rejected\n-# {}",
            outcome_line(resolved_by, "Rejected by", voters(VoteKind::Reject))
        ),
        ReviewStatus::Voided if review.voided_by == Some(review.submitter_id) => {
            "### Voided\n-# Withdrawn by the submitter.".to_string()
        }
        ReviewStatus::Voided => format!(
            "### Voided\n-# Voided by <@{}>",
            review.voided_by.unwrap_or_default()
        ),
        ReviewStatus::Draft | ReviewStatus::Open | ReviewStatus::Expired => String::new(),
    }
}

fn outcome_line(resolved_by: Option<String>, verb: &str, voters: Vec<String>) -> String {
    let named = (!voters.is_empty()).then(|| format!("{verb} {}.", voters.join(", ")));
    [resolved_by, named]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tag_card(loaded: &Loaded, face_url: &str) -> CreateContainerComponent<'static> {
    let added = (!loaded.review.hide_username)
        .then(|| format!("> -# **\\- Added by `@{}`**", loaded.submitter_name));
    let block = format_tag_block(
        &loaded.revision.tag_type,
        &sanitize_reason(&loaded.revision.reason),
        "",
        added.as_deref(),
        None,
        false,
    );
    let uuid_line = format!(
        "-# UUID: {}",
        format_uuid_dashed(&loaded.review.player_uuid)
    );
    CreateContainerComponent::Section(CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            format!("{block}\n{uuid_line}"),
        ))],
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            face_url.to_string(),
        ))),
    ))
}

fn media_gallery(
    evidence: &[Evidence],
    urls: &MediaUrls,
) -> Option<CreateContainerComponent<'static>> {
    let items: Vec<CreateMediaGalleryItem> = evidence
        .iter()
        .filter(|e| e.kind == EvidenceKind::Media)
        .filter_map(|e| {
            let url = urls
                .evidence
                .get(&e.id)
                .cloned()
                .or_else(|| e.filename.as_ref().map(|f| format!("attachment://{f}")))?;
            Some(CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(
                url,
            )))
        })
        .collect();
    (!items.is_empty())
        .then(|| CreateContainerComponent::MediaGallery(CreateMediaGallery::new(items)))
}

fn replay_lines(evidence: &[Evidence]) -> Option<String> {
    let lines: Vec<String> = evidence
        .iter()
        .filter(|e| e.kind == EvidenceKind::Replay)
        .map(replay_line)
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn evidence_list(evidence: &[Evidence]) -> Option<String> {
    let lines: Vec<String> = evidence
        .iter()
        .enumerate()
        .map(|(i, piece)| format!("{}. {}", i + 1, piece_line(piece)))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn piece_line(piece: &Evidence) -> String {
    match piece.kind {
        EvidenceKind::Replay => replay_line(piece).trim_start_matches("- ").to_string(),
        EvidenceKind::Media => {
            let label = piece
                .filename
                .as_deref()
                .map(media_label)
                .unwrap_or_default();
            with_note(format!("Media `{label}`"), piece.note.as_deref())
        }
    }
}

fn replay_line(piece: &Evidence) -> String {
    let command = replay_command(piece);
    with_note(format!("- `{command}`"), piece.note.as_deref())
}

pub fn replay_command(piece: &Evidence) -> String {
    Replay {
        id: piece.replay_id.clone().unwrap_or_default(),
        timestamp: piece.replay_ts.clone(),
    }
    .format_command()
}

fn with_note(line: String, note: Option<&str>) -> String {
    match note {
        Some(note) => format!("{line} Note: \"{}\"", sanitize_reason(note)),
        None => line,
    }
}

fn piece_label(evidence: &[Evidence], piece: &Evidence) -> String {
    match piece.kind {
        EvidenceKind::Media => piece
            .filename
            .as_deref()
            .map(media_label)
            .unwrap_or_default(),
        EvidenceKind::Replay => {
            let n = evidence
                .iter()
                .filter(|e| e.kind == EvidenceKind::Replay && e.position <= piece.position)
                .count();
            format!("Replay {n}")
        }
    }
}

pub fn simple(lines: impl Into<String>) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(lines.into()),
    ]))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_line_joins_resolver_and_voters() {
        assert_eq!(
            outcome_line(
                Some("Resolved by <@1>.".into()),
                "Accepted by",
                vec!["<@2>".into()]
            ),
            "Resolved by <@1>. Accepted by <@2>."
        );
        assert_eq!(outcome_line(None, "Rejected by", vec![]), "");
    }
}
