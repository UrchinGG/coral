use serenity::all::*;

use database::{EvidencePost, EvidencePostRepository};

use crate::framework::Data;
use crate::utils::format_uuid_dashed;

pub mod confirm;
pub mod convert;
pub mod index;
pub mod media;
pub mod menu;
pub mod ops;
pub mod render;

pub const FACE_FILENAME: &str = "face.png";
pub const MAX_EVIDENCE_ITEMS: usize = 9;
pub const MAX_FILES_PER_UPLOAD: u8 = 4;
pub const ALLOWED_MEDIA_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "mp4", "webm", "mov"];
const FACE_SIZE: u32 = 128;

pub async fn face_attachment(data: &Data, uuid: &str) -> CreateAttachment<'static> {
    let png = data
        .skin_provider
        .fetch_face(uuid, FACE_SIZE)
        .await
        .unwrap_or_else(blank_face);
    CreateAttachment::bytes(png, FACE_FILENAME)
}

fn blank_face() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(FACE_SIZE, FACE_SIZE, image::Rgba([0, 0, 0, 0]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encoding a blank png cannot fail");
    buf.into_inner()
}

pub fn thread_title(username: &str, uuid: &str) -> String {
    format!("{username} | {}", format_uuid_dashed(uuid))
}

pub fn thread_link(data: &Data, thread_id: i64) -> String {
    match data.home_guild_id {
        Some(guild_id) => format!("https://discord.com/channels/{guild_id}/{thread_id}"),
        None => format!("<#{thread_id}>"),
    }
}

pub async fn evidence_thread_url(data: &Data, uuid: &str) -> Option<String> {
    let post = link_target(data, uuid).await?;
    data.home_guild_id
        .map(|guild_id| format!("https://discord.com/channels/{guild_id}/{}", post.thread_id))
}

pub async fn link_target(data: &Data, uuid: &str) -> Option<EvidencePost> {
    EvidencePostRepository::new(data.db.pool())
        .link_target(&normalize(uuid))
        .await
        .inspect_err(|e| tracing::error!("Failed to look up evidence post for {uuid}: {e}"))
        .ok()
        .flatten()
}

pub fn normalize(uuid: &str) -> String {
    uuid.replace('-', "").to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Open,
    Locked,
    Missing,
}

pub async fn reopen_if_idle(ctx: &Context, thread_id: i64) -> ThreadState {
    let Ok(channel) = ctx
        .http
        .get_channel(GenericChannelId::new(thread_id as u64))
        .await
    else {
        return ThreadState::Missing;
    };
    let Some(thread) = channel.thread() else {
        return ThreadState::Missing;
    };
    if thread.thread_metadata.locked() {
        return ThreadState::Locked;
    }
    if thread.thread_metadata.archived()
        && let Err(e) = ThreadId::new(thread_id as u64)
            .edit(&ctx.http, EditThread::new().archived(false))
            .await
    {
        tracing::warn!("Could not reopen idle evidence thread {thread_id}: {e}");
    }
    ThreadState::Open
}

pub fn media_extension(filename: &str) -> String {
    filename
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub fn is_media_filename(filename: &str) -> bool {
    ALLOWED_MEDIA_EXTENSIONS.contains(&media_extension(filename).as_str())
}

pub fn media_label(filename: &str) -> String {
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    match stem.rsplit_once('_') {
        Some((name, n)) => format!("{name} ({n})"),
        None => stem.to_string(),
    }
}
