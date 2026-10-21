use serenity::all::*;

use database::{EvidencePost, EvidencePostRepository, Origin, PostStatus, get_locked, set_status};

use super::convert::{self, Conversion, Credit};
use super::media::{self, Change, EditError, EditReport, NewFile, View, edit_post};
use super::{MAX_EVIDENCE_ITEMS, render};
use crate::framework::Data;
use crate::utils::resolve_username_or_fetch;

#[derive(Debug)]
pub enum OpError {
    NoPost,
    Archived,
    Full { room: usize },
    Edit(EditError),
    Database(anyhow::Error),
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPost => write!(f, "That evidence post no longer exists."),
            Self::Archived => write!(
                f,
                "That evidence post is archived. Run /confirm on the player to restore it."
            ),
            Self::Full { room: 0 } => write!(
                f,
                "This evidence post already holds the maximum of {MAX_EVIDENCE_ITEMS} files."
            ),
            Self::Full { room } => write!(
                f,
                "Only {room} more files fit on this evidence post, so nothing was added."
            ),
            Self::Edit(e) => write!(f, "{e}"),
            Self::Database(e) => write!(f, "Something went wrong saving the evidence post ({e})."),
        }
    }
}

impl std::error::Error for OpError {}

impl From<EditError> for OpError {
    fn from(e: EditError) -> Self {
        Self::Edit(e)
    }
}

impl From<database::SqlxError> for OpError {
    fn from(e: database::SqlxError) -> Self {
        Self::Database(e.into())
    }
}

pub struct UploadResult {
    pub report: EditReport,
    pub conversion: Conversion,
}

pub async fn upload(
    ctx: &Context,
    data: &Data,
    thread_id: i64,
    files: Vec<(String, Vec<u8>)>,
    actor: u64,
    credit: Credit,
) -> Result<UploadResult, OpError> {
    let repo = EvidencePostRepository::new(data.db.pool());
    let post = repo.get(thread_id).await?.ok_or(OpError::NoPost)?;
    let mut tx = repo.begin_locked(&post.uuid).await?;
    let post = get_locked(&mut tx, thread_id)
        .await?
        .ok_or(OpError::NoPost)?;
    if post.origin != Origin::Coral {
        return Err(OpError::NoPost);
    }
    if post.status == Some(PostStatus::Archived) {
        return Err(OpError::Archived);
    }
    if super::reopen_if_idle(ctx, thread_id).await != super::ThreadState::Open {
        return Err(OpError::Archived);
    }
    let existing = media::gallery_files(&media::current_media(ctx, thread_id).await?);
    let room = MAX_EVIDENCE_ITEMS.saturating_sub(existing.len());
    if files.len() > room {
        return Err(OpError::Full { room });
    }
    let username = username(data, &post).await;
    let extensions: Vec<String> = files.iter().map(|(ext, _)| ext.clone()).collect();
    let names = media::next_filenames(&username, &existing, &extensions);
    let new_files = names
        .into_iter()
        .zip(files)
        .map(|(filename, (_, bytes))| NewFile { filename, bytes })
        .collect();
    let tag = render::tag_view(ctx, data, &post)
        .await
        .map_err(OpError::Database)?;
    let report = edit_post(
        ctx,
        data,
        &post,
        &username,
        &tag,
        View::Live,
        Change::Add(new_files),
    )
    .await?;
    let conversion = convert::run(
        &mut tx,
        data,
        &post,
        !report.files.is_empty(),
        actor,
        credit,
    )
    .await
    .map_err(OpError::Database)?;
    if conversion == Conversion::Converted {
        let post = get_locked(&mut tx, thread_id)
            .await?
            .ok_or(OpError::NoPost)?;
        let tag = render::tag_view(ctx, data, &post)
            .await
            .map_err(OpError::Database)?;
        edit_post(
            ctx,
            data,
            &post,
            &username,
            &tag,
            View::Live,
            Change::Rerender,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(UploadResult { report, conversion })
}

pub async fn remove(
    ctx: &Context,
    data: &Data,
    thread_id: i64,
    attachment_id: AttachmentId,
) -> Result<EditReport, OpError> {
    let repo = EvidencePostRepository::new(data.db.pool());
    let post = repo.get(thread_id).await?.ok_or(OpError::NoPost)?;
    let mut tx = repo.begin_locked(&post.uuid).await?;
    let post = get_locked(&mut tx, thread_id)
        .await?
        .ok_or(OpError::NoPost)?;
    if post.status == Some(PostStatus::Archived) {
        return Err(OpError::Archived);
    }
    super::reopen_if_idle(ctx, thread_id).await;
    let username = username(data, &post).await;
    let tag = render::tag_view(ctx, data, &post)
        .await
        .map_err(OpError::Database)?;
    let report = edit_post(
        ctx,
        data,
        &post,
        &username,
        &tag,
        View::Live,
        Change::Remove(attachment_id),
    )
    .await?;
    tx.commit().await?;
    Ok(report)
}

pub async fn refresh_for_player(ctx: &Context, data: &Data, uuid: &str) -> Result<(), OpError> {
    let repo = EvidencePostRepository::new(data.db.pool());
    let Some(post) = repo.canonical(&super::normalize(uuid)).await? else {
        return Ok(());
    };
    if post.status == Some(PostStatus::Archived) {
        return Ok(());
    }
    let tx = repo.begin_locked(&post.uuid).await?;
    super::reopen_if_idle(ctx, post.thread_id).await;
    let username = username(data, &post).await;
    let tag = render::tag_view(ctx, data, &post)
        .await
        .map_err(OpError::Database)?;
    edit_post(
        ctx,
        data,
        &post,
        &username,
        &tag,
        View::Live,
        Change::Rerender,
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn archive_for_player(ctx: &Context, data: &Data, uuid: &str) -> Result<(), OpError> {
    let repo = EvidencePostRepository::new(data.db.pool());
    match repo.canonical(&super::normalize(uuid)).await? {
        Some(post) if post.status == Some(PostStatus::Confirmed) => {
            archive(ctx, data, post.thread_id).await
        }
        _ => Ok(()),
    }
}

pub async fn archive(ctx: &Context, data: &Data, thread_id: i64) -> Result<(), OpError> {
    let repo = EvidencePostRepository::new(data.db.pool());
    let post = repo.get(thread_id).await?.ok_or(OpError::NoPost)?;
    let mut tx = repo.begin_locked(&post.uuid).await?;
    let post = get_locked(&mut tx, thread_id)
        .await?
        .ok_or(OpError::NoPost)?;
    if post.status == Some(PostStatus::Archived) {
        return Ok(());
    }
    super::reopen_if_idle(ctx, thread_id).await;
    let username = username(data, &post).await;
    let tag = render::tag_view(ctx, data, &post)
        .await
        .map_err(OpError::Database)?;
    edit_post(
        ctx,
        data,
        &post,
        &username,
        &tag,
        View::Archived,
        Change::Rerender,
    )
    .await?;
    set_status(&mut tx, thread_id, PostStatus::Archived).await?;
    tx.commit().await?;
    ThreadId::new(thread_id as u64)
        .edit(&ctx.http, EditThread::new().archived(true).locked(true))
        .await
        .map_err(|e| OpError::Edit(EditError::Discord(e)))?;
    Ok(())
}

pub async fn restore(
    ctx: &Context,
    data: &Data,
    thread_id: i64,
    actor: u64,
) -> Result<Conversion, OpError> {
    let repo = EvidencePostRepository::new(data.db.pool());
    let post = repo.get(thread_id).await?.ok_or(OpError::NoPost)?;
    let mut tx = repo.begin_locked(&post.uuid).await?;
    let post = get_locked(&mut tx, thread_id)
        .await?
        .ok_or(OpError::NoPost)?;
    ThreadId::new(thread_id as u64)
        .edit(&ctx.http, EditThread::new().archived(false).locked(false))
        .await
        .map_err(|e| OpError::Edit(EditError::Discord(e)))?;
    set_status(&mut tx, thread_id, PostStatus::Pending).await?;
    let username = username(data, &post).await;
    let tag = render::tag_view(ctx, data, &post)
        .await
        .map_err(OpError::Database)?;
    let report = edit_post(
        ctx,
        data,
        &post,
        &username,
        &tag,
        View::Live,
        Change::Rerender,
    )
    .await?;
    let conversion = convert::run(
        &mut tx,
        data,
        &post,
        !report.files.is_empty(),
        actor,
        Credit::Confirmer,
    )
    .await
    .map_err(OpError::Database)?;
    if conversion == Conversion::Converted {
        let post = get_locked(&mut tx, thread_id)
            .await?
            .ok_or(OpError::NoPost)?;
        let tag = render::tag_view(ctx, data, &post)
            .await
            .map_err(OpError::Database)?;
        edit_post(
            ctx,
            data,
            &post,
            &username,
            &tag,
            View::Live,
            Change::Rerender,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(conversion)
}

pub async fn username(data: &Data, post: &EvidencePost) -> String {
    resolve_username_or_fetch(&post.uuid, data)
        .await
        .unwrap_or_else(|| post.player_name().to_string())
}
