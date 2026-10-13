use std::collections::HashMap;

use serenity::all::*;

use database::EvidencePost;

use super::render::{self, TagView};
use super::{FACE_FILENAME, face_attachment};
use crate::framework::Data;
use crate::utils::discord_media::{MessageMedia, message_media};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Live,
    Archived,
}

pub struct NewFile {
    pub filename: String,
    pub bytes: Vec<u8>,
}

pub enum Change {
    Add(Vec<NewFile>),
    Remove(AttachmentId),
    Rerender,
}

#[derive(Debug, Default)]
pub struct EditReport {
    pub files: Vec<String>,
    pub added: usize,
    pub reattached: Vec<String>,
    pub lost: Vec<String>,
}

#[derive(Debug)]
pub enum EditError {
    Unreadable(String),
    Download(Vec<String>),
    Discord(serenity::Error),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(detail) => write!(
                f,
                "The evidence post could not be read safely, so nothing was changed. ({detail})"
            ),
            Self::Download(files) => write!(
                f,
                "These files could not be downloaded, so nothing was changed: {}",
                files.join(", ")
            ),
            Self::Discord(e) => write!(
                f,
                "Discord rejected the edit, so nothing was changed. ({e})"
            ),
        }
    }
}

pub async fn current_media(ctx: &Context, thread_id: i64) -> Result<Vec<MessageMedia>, EditError> {
    message_media(
        &ctx.http,
        GenericChannelId::new(thread_id as u64),
        MessageId::new(thread_id as u64),
    )
    .await
    .map_err(|e| EditError::Unreadable(e.to_string()))
}

pub fn gallery_files(media: &[MessageMedia]) -> Vec<String> {
    media
        .iter()
        .filter(|m| m.in_gallery)
        .map(|m| m.filename.clone())
        .collect()
}

pub async fn edit_post(
    ctx: &Context,
    data: &Data,
    post: &EvidencePost,
    username: &str,
    tag: &TagView,
    view: View,
    change: Change,
) -> Result<EditReport, EditError> {
    let current = current_media(ctx, post.thread_id).await?;
    let gallery: Vec<&MessageMedia> = current.iter().filter(|m| m.in_gallery).collect();
    let face = current.iter().find(|m| !m.in_gallery);
    let rewrites_files = !matches!(change, Change::Rerender);
    let held = if rewrites_files {
        hold_files(&gallery).await?
    } else {
        HashMap::new()
    };

    let (removed, new_files) = match change {
        Change::Add(files) => (None, files),
        Change::Remove(id) => (Some(id), Vec::new()),
        Change::Rerender => (None, Vec::new()),
    };
    let kept: Vec<&MessageMedia> = gallery
        .iter()
        .copied()
        .filter(|m| Some(m.attachment_id) != removed)
        .collect();
    let mut files: Vec<String> = kept.iter().map(|m| m.filename.clone()).collect();
    files.extend(new_files.iter().map(|f| f.filename.clone()));

    let components = match view {
        View::Live => render::live(username, &post.uuid, tag, &files),
        View::Archived => render::archived(username, &post.uuid, &files),
    };
    let mut attachments = EditAttachments::new();
    for media in &kept {
        attachments = attachments.keep(media.attachment_id);
    }
    match face {
        Some(face) => attachments = attachments.keep(face.attachment_id),
        None => attachments = attachments.add(face_attachment(data, &post.uuid).await),
    }
    for file in &new_files {
        attachments = attachments.add(CreateAttachment::bytes(
            file.bytes.clone(),
            file.filename.clone(),
        ));
    }
    write(ctx, post.thread_id, components.clone(), attachments).await?;

    let mut report = EditReport {
        files: files.clone(),
        added: new_files.len(),
        ..Default::default()
    };
    if rewrites_files {
        verify(
            ctx,
            data,
            post,
            &kept,
            &new_files,
            &held,
            components,
            &mut report,
        )
        .await?;
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
async fn verify(
    ctx: &Context,
    data: &Data,
    post: &EvidencePost,
    kept: &[&MessageMedia],
    new_files: &[NewFile],
    held: &HashMap<u64, Vec<u8>>,
    components: Vec<CreateComponent<'static>>,
    report: &mut EditReport,
) -> Result<(), EditError> {
    let after = current_media(ctx, post.thread_id).await?;
    let present_ids: Vec<u64> = after.iter().map(|m| m.attachment_id.get()).collect();
    let present_names: Vec<&str> = after.iter().map(|m| m.filename.as_str()).collect();
    let missing: Vec<&MessageMedia> = kept
        .iter()
        .copied()
        .filter(|m| !present_ids.contains(&m.attachment_id.get()))
        .collect();
    report.lost.extend(
        new_files
            .iter()
            .filter(|f| !present_names.contains(&f.filename.as_str()))
            .map(|f| f.filename.clone()),
    );
    if missing.is_empty() {
        return Ok(());
    }

    let mut attachments = EditAttachments::new();
    for media in &after {
        attachments = attachments.keep(media.attachment_id);
    }
    for media in &missing {
        if let Some(bytes) = held.get(&media.attachment_id.get()) {
            attachments = attachments.add(CreateAttachment::bytes(
                bytes.clone(),
                media.filename.clone(),
            ));
        }
    }
    if !after.iter().any(|m| m.filename == FACE_FILENAME) {
        attachments = attachments.add(face_attachment(data, &post.uuid).await);
    }
    match write(ctx, post.thread_id, components, attachments).await {
        Ok(()) => report
            .reattached
            .extend(missing.iter().map(|m| m.filename.clone())),
        Err(e) => {
            tracing::error!(
                "Re-attaching evidence on thread {} failed: {e}",
                post.thread_id
            );
            report
                .lost
                .extend(missing.iter().map(|m| m.filename.clone()));
        }
    }
    Ok(())
}

async fn hold_files(gallery: &[&MessageMedia]) -> Result<HashMap<u64, Vec<u8>>, EditError> {
    let mut held = HashMap::new();
    let mut failed = Vec::new();
    for media in gallery {
        match download(&media.url).await {
            Some(bytes) => {
                held.insert(media.attachment_id.get(), bytes);
            }
            None => failed.push(media.filename.clone()),
        }
    }
    if failed.is_empty() {
        Ok(held)
    } else {
        Err(EditError::Download(failed))
    }
}

pub async fn download(url: &str) -> Option<Vec<u8>> {
    let response = reqwest::get(url).await.ok()?.error_for_status().ok()?;
    response.bytes().await.ok().map(|b| b.to_vec())
}

async fn write(
    ctx: &Context,
    thread_id: i64,
    components: Vec<CreateComponent<'static>>,
    attachments: EditAttachments<'static>,
) -> Result<(), EditError> {
    ctx.http
        .edit_message(
            GenericChannelId::new(thread_id as u64),
            MessageId::new(thread_id as u64),
            &EditMessage::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(components)
                .attachments(attachments),
            Vec::new(),
        )
        .await
        .map(|_| ())
        .map_err(EditError::Discord)
}

pub fn next_filenames(username: &str, existing: &[String], extensions: &[String]) -> Vec<String> {
    let start = existing
        .iter()
        .filter_map(|f| {
            let stem = f.rsplit_once('.').map_or(f.as_str(), |(stem, _)| stem);
            stem.rsplit_once('_')?.1.parse::<usize>().ok()
        })
        .max()
        .unwrap_or(0)
        + 1;
    extensions
        .iter()
        .enumerate()
        .map(|(i, ext)| format!("{username}_{}.{ext}", start + i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_filenames_continue_after_the_highest_number() {
        let existing = vec!["Hexze_1.png".to_string(), "Hexze_4.mp4".to_string()];
        assert_eq!(
            next_filenames("Hexze", &existing, &["png".into(), "mov".into()]),
            vec!["Hexze_5.png", "Hexze_6.mov"]
        );
        assert_eq!(
            next_filenames("Kai", &[], &["png".into()]),
            vec!["Kai_1.png"]
        );
    }
}
