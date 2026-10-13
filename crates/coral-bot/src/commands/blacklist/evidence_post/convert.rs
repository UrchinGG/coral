use coral_redis::BlacklistEvent;

use database::{
    AddOutcome, BlacklistRepository, EvidencePost, OverwriteOutcome, PgConnection, PostStatus,
    set_pending, set_status,
};

use crate::framework::Data;

const BASE_TAGS: &[&str] = &["closet_cheater", "blatant_cheater"];

#[derive(Debug, PartialEq, Eq)]
pub enum Conversion {
    Converted,
    AlreadyConfirmed,
    NotApplicable,
    Failed(String),
}

pub async fn run(
    conn: &mut PgConnection,
    data: &Data,
    post: &EvidencePost,
    has_media: bool,
    actor: u64,
) -> anyhow::Result<Conversion> {
    let repo = BlacklistRepository::new(data.db.pool());
    let tags = repo.get_active_tags(&post.uuid).await?;
    if tags
        .iter()
        .any(|t| t.tag_type.as_deref() == Some("confirmed_cheater"))
    {
        set_status(conn, post.thread_id, PostStatus::Confirmed).await?;
        return Ok(Conversion::AlreadyConfirmed);
    }
    if !has_media {
        return Ok(Conversion::NotApplicable);
    }
    let base = tags.iter().find(|t| {
        t.tag_type
            .as_deref()
            .is_some_and(|ty| BASE_TAGS.contains(&ty))
    });
    let outcome = match (base, &post.pending) {
        (Some(tag), _) => overwrite(data, tag, actor, true).await?,
        (None, Some(pending)) => {
            add_then_overwrite(
                data,
                post,
                &pending.tag_type,
                &pending.reason,
                pending.by,
                actor,
            )
            .await?
        }
        (None, None) => return Ok(Conversion::NotApplicable),
    };
    if outcome == Conversion::Converted {
        set_status(conn, post.thread_id, PostStatus::Confirmed).await?;
        set_pending(conn, post.thread_id, None).await?;
    }
    Ok(outcome)
}

async fn overwrite(
    data: &Data,
    tag: &database::PlayerEvent,
    actor: u64,
    announce_overwrite: bool,
) -> anyhow::Result<Conversion> {
    let repo = BlacklistRepository::new(data.db.pool());
    let old_tag_type = tag.tag_type.clone().unwrap_or_default();
    let old_reason = tag.reason.clone().unwrap_or_default();
    let blocking = blacklist::priority_lane_excluding("confirmed_cheater", &old_tag_type);
    let outcome = repo
        .overwrite_event(
            &tag.uuid,
            &old_tag_type,
            "confirmed_cheater",
            &old_reason,
            tag.hide_username.unwrap_or(false),
            None,
            None,
            Some(actor as i64),
            Some(actor as i64),
            None,
            &blocking,
            Some(tag.id),
        )
        .await?;
    match outcome {
        OverwriteOutcome::Inserted { new, .. } => {
            let event = if announce_overwrite {
                BlacklistEvent::TagOverwritten {
                    uuid: tag.uuid.clone(),
                    old_tag_id: tag.id,
                    old_tag_type,
                    old_reason,
                    new_tag_id: new.id,
                    overwritten_by: actor as i64,
                    silent: false,
                }
            } else {
                BlacklistEvent::TagAdded {
                    uuid: tag.uuid.clone(),
                    tag_id: new.id,
                    added_by: actor as i64,
                    silent: false,
                    review_url: None,
                }
            };
            data.event_publisher.publish(&event).await;
            Ok(Conversion::Converted)
        }
        OverwriteOutcome::OldNotActive => Ok(Conversion::Failed(
            "The tag changed while it was being confirmed.".to_string(),
        )),
        OverwriteOutcome::Conflict(conflict) => Ok(Conversion::Failed(format!(
            "Conflicts with an existing tag ({}).",
            conflict.tag_type.unwrap_or_default()
        ))),
    }
}

async fn add_then_overwrite(
    data: &Data,
    post: &EvidencePost,
    tag_type: &str,
    reason: &str,
    pending_by: i64,
    actor: u64,
) -> anyhow::Result<Conversion> {
    let repo = BlacklistRepository::new(data.db.pool());
    let blocking = blacklist::priority_lane_excluding(tag_type, "");
    let added = repo
        .add_event(
            &post.uuid,
            tag_type,
            reason,
            false,
            None,
            None,
            Some(pending_by),
            &blocking,
        )
        .await?;
    let AddOutcome::Inserted(tag_id) = added else {
        return Ok(Conversion::Failed(
            "Conflicts with an existing tag.".to_string(),
        ));
    };
    let written = repo
        .get_active_tag(&post.uuid, tag_type)
        .await?
        .filter(|t| t.id == tag_id);
    let result = match &written {
        Some(tag) => overwrite(data, tag, actor, false).await,
        None => Ok(Conversion::Failed(
            "The new tag could not be read back.".to_string(),
        )),
    };
    if matches!(result, Ok(Conversion::Converted)) {
        return Ok(Conversion::Converted);
    }
    let cause = match result {
        Ok(Conversion::Failed(cause)) => cause,
        Ok(_) => "The tag could not be confirmed.".to_string(),
        Err(e) => format!("The tag could not be confirmed ({e})."),
    };
    let undone = repo
        .remove_event(&post.uuid, tag_type, Some(pending_by))
        .await
        .unwrap_or(false);
    Ok(Conversion::Failed(if undone {
        format!("{cause} Nothing changed; the next upload retries.")
    } else {
        format!(
            "{cause} Undoing the first write also failed, so the tag stays and converts on the next upload."
        )
    }))
}
