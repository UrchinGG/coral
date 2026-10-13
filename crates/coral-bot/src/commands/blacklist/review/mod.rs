use std::collections::HashMap;

use chrono::Duration;
use serenity::all::*;

use database::{Evidence, Review, ReviewRepository, Revision};

use crate::framework::Data;
use crate::utils::resolve_username_or_fetch;

pub mod community;
pub mod console;
pub mod deadline;
pub mod edit;
pub mod guide;
pub mod publish;
pub mod render;
pub mod resolve;
pub mod staff_ping;
pub mod void;
pub mod vote;

pub const ID_PREFIX: &str = "rv:";
pub const REVIEW_TAGS: &[&str] = &["closet_cheater", "blatant_cheater"];
pub const MAX_MEDIA: i64 = 9;
pub const MAX_FILES_PER_UPLOAD: u8 = 4;
pub const NOTE_MAX_CHARS: u16 = 75;
pub const REASON_MAX_CHARS: u16 = 120;
pub const REVISE_MAX_CHARS: u16 = 300;
pub const ALLOWED_MEDIA_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm", "mov"];
pub const FACE_SIZE: u32 = 128;

const FORUM_TAG_PENDING: &str = "Pending";
const FORUM_TAG_APPROVED: &str = "Approved";
const FORUM_TAG_REJECTED: &str = "Rejected";
const FORUM_TAG_VOIDED: &str = "Voided";

pub fn draft_idle_window() -> Duration {
    Duration::minutes(30)
}

pub fn edit_idle_window() -> Duration {
    Duration::minutes(3)
}

pub fn custom_id(action: &str, review_id: i64) -> String {
    format!("{ID_PREFIX}{action}:{review_id}")
}

pub fn custom_id_with(action: &str, review_id: i64, extra: impl std::fmt::Display) -> String {
    format!("{ID_PREFIX}{action}:{review_id}:{extra}")
}

pub struct ParsedId<'a> {
    pub action: &'a str,
    pub review_id: i64,
    pub extra: Option<&'a str>,
}

pub fn parse_id(custom_id: &str) -> Option<ParsedId<'_>> {
    let rest = custom_id.strip_prefix(ID_PREFIX)?;
    let mut parts = rest.splitn(3, ':');
    Some(ParsedId {
        action: parts.next()?,
        review_id: parts.next()?.parse().ok()?,
        extra: parts.next(),
    })
}

const VOTE_ACTIONS: &[&str] = &["accept", "reject", "revise", "switch", "addvote", "resolve"];

pub async fn handle_component(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    let Some(id) = parse_id(&component.data.custom_id) else {
        return Ok(());
    };
    if id.action == "void_go" {
        return void::confirm(ctx, component, data, id.review_id).await;
    }
    if id.action == "confirmtag" {
        return staff_ping::handle_click(ctx, component, data, id.review_id).await;
    }
    if id.action == "editpost" {
        return edit::handle_edit_click(ctx, component, data, id.review_id).await;
    }
    if community::ACTIONS.contains(&id.action) {
        return community::handle_component(
            ctx,
            component,
            data,
            id.action,
            id.review_id,
            id.extra,
        )
        .await;
    }
    if VOTE_ACTIONS.contains(&id.action) {
        return vote::handle_component(ctx, component, data, id.action, id.review_id, id.extra)
            .await;
    }
    console::handle_component(ctx, component, data).await
}

pub async fn handle_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> anyhow::Result<()> {
    match parse_id(&modal.data.custom_id) {
        Some(id) if id.action == "revise_m" => {
            vote::handle_revise_modal(ctx, modal, data, id.review_id).await
        }
        Some(_) => console::handle_modal(ctx, modal, data).await,
        None => Ok(()),
    }
}

pub async fn reply_retired_button(
    ctx: &Context,
    component: &ComponentInteraction,
) -> anyhow::Result<()> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .ephemeral(true)
                    .content("This button no longer works. Run the command again."),
            ),
        )
        .await?;
    Ok(())
}

pub fn face_filename(uuid: &str) -> String {
    format!("face_{uuid}.png")
}

pub async fn face_attachment(data: &Data, uuid: &str) -> CreateAttachment<'static> {
    let png = crate::utils::face::face_png(data, uuid, FACE_SIZE)
        .await
        .unwrap_or_else(blank_face);
    CreateAttachment::bytes(png, face_filename(uuid))
}

fn blank_face() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(FACE_SIZE, FACE_SIZE, image::Rgba([0, 0, 0, 0]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encoding a blank png cannot fail");
    buf.into_inner()
}

pub struct Loaded {
    pub review: Review,
    pub revision: Revision,
    pub evidence: Vec<Evidence>,
    pub username: String,
    pub submitter_name: String,
}

pub async fn load_working(
    ctx: &Context,
    data: &Data,
    review_id: i64,
) -> anyhow::Result<Option<Loaded>> {
    let repo = ReviewRepository::new(data.db.pool());
    let Some(review) = repo.get(review_id).await? else {
        return Ok(None);
    };
    let Some(revision) = repo.working_revision(review_id).await? else {
        return Ok(None);
    };
    finish_load(ctx, data, review, revision).await.map(Some)
}

pub async fn load_live(
    ctx: &Context,
    data: &Data,
    review_id: i64,
) -> anyhow::Result<Option<Loaded>> {
    let repo = ReviewRepository::new(data.db.pool());
    let Some(review) = repo.get(review_id).await? else {
        return Ok(None);
    };
    let Some(revision) = repo.live_revision(review_id).await? else {
        return Ok(None);
    };
    finish_load(ctx, data, review, revision).await.map(Some)
}

async fn finish_load(
    ctx: &Context,
    data: &Data,
    review: Review,
    revision: Revision,
) -> anyhow::Result<Loaded> {
    let evidence = ReviewRepository::new(data.db.pool())
        .evidence(review.id, revision.revision)
        .await?;
    let username = resolve_username_or_fetch(&review.player_uuid, data)
        .await
        .unwrap_or_else(|| review.player_uuid.clone());
    let submitter_name = super::channel::get_username(ctx, review.submitter_id as u64).await;
    Ok(Loaded {
        review,
        revision,
        evidence,
        username,
        submitter_name,
    })
}

pub struct ForumTags {
    by_name: HashMap<String, ForumTagId>,
}

impl ForumTags {
    pub async fn resolve(ctx: &Context, data: &Data) -> Self {
        let mut by_name = HashMap::new();
        if let Some(forum_id) = data.review_forum_id
            && let Ok(Channel::Guild(forum)) = ctx.http.get_channel(forum_id.into()).await
        {
            for tag in forum.available_tags.iter() {
                by_name.insert(tag.name.to_string(), tag.id);
            }
        }
        Self { by_name }
    }

    pub fn for_status(&self, status: database::ReviewStatus) -> Vec<ForumTagId> {
        use database::ReviewStatus::*;
        let name = match status {
            Open => FORUM_TAG_PENDING,
            Accepted => FORUM_TAG_APPROVED,
            Rejected => FORUM_TAG_REJECTED,
            Voided => FORUM_TAG_VOIDED,
            Draft | Expired => return Vec::new(),
        };
        self.by_name.get(name).copied().into_iter().collect()
    }
}

pub fn media_label(filename: &str) -> String {
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    match stem.rsplit_once('_') {
        Some((name, n)) => format!("{name} ({n})"),
        None => stem.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_ids_round_trip() {
        let id = custom_id_with("evnote", 42, 7);
        let parsed = parse_id(&id).unwrap();
        assert_eq!(
            (parsed.action, parsed.review_id, parsed.extra),
            ("evnote", 42, Some("7"))
        );
        assert!(parse_id("review_approve:0:1").is_none());
    }

    #[test]
    fn media_labels_number_the_player() {
        assert_eq!(media_label("Hexze_2.mp4"), "Hexze (2)");
    }
}
