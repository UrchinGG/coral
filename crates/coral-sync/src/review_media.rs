use serenity::all::*;

use coral_redis::{SyncEvent, SyncEventPublisher, ThreadAttachment};
use database::{ReviewRepository, ReviewStatus};

use crate::framework::Data;

pub fn relay_thread_media(data: &Data, message: &Message) {
    if message.author.bot() || message.attachments.is_empty() {
        return;
    }
    let Some(redis) = data.redis.clone() else {
        return;
    };
    let data = data.clone();
    let thread_id = message.channel_id.get();
    let message_id = message.id.get();
    let author_id = message.author.id.get();
    let attachments: Vec<ThreadAttachment> = message
        .attachments
        .iter()
        .map(|a| ThreadAttachment {
            url: a.url.to_string(),
            filename: a.filename.to_string(),
            content_type: a.content_type.as_ref().map(|c| c.to_string()),
        })
        .collect();
    tokio::spawn(async move {
        let review = match ReviewRepository::new(data.db.pool())
            .get_by_thread(thread_id as i64)
            .await
        {
            Ok(Some(review)) if review.status == ReviewStatus::Open => review,
            Ok(_) => return,
            Err(e) => {
                tracing::warn!("Review lookup failed for thread {thread_id}: {e}");
                return;
            }
        };
        SyncEventPublisher::new(redis)
            .publish(&SyncEvent::ReviewThreadMedia {
                review_id: review.id,
                thread_id,
                message_id,
                author_id,
                attachments,
            })
            .await;
    });
}
