use serenity::all::*;

use coral_redis::{SyncEvent, SyncEventSubscriber};

use crate::commands::blacklist::review::community;
use crate::framework::Data;

pub fn spawn_sync_subscriber(ctx: Context, data: Data) {
    let redis_url = data.redis_url.clone();

    tokio::spawn(async move {
        loop {
            let ctx = ctx.clone();
            let data = data.clone();

            let result = SyncEventSubscriber::run(&redis_url, move |event| {
                let ctx = ctx.clone();
                let data = data.clone();
                async move {
                    let SyncEvent::ReviewThreadMedia {
                        review_id,
                        thread_id,
                        message_id,
                        author_id,
                        attachments,
                    } = event
                    else {
                        return;
                    };
                    if let Err(e) = community::prompt(
                        &ctx,
                        &data,
                        review_id,
                        thread_id,
                        message_id,
                        author_id,
                        attachments,
                    )
                    .await
                    {
                        tracing::error!("Failed to prompt for media in review {review_id}: {e:?}");
                    }
                }
            })
            .await;

            if let Err(e) = result {
                tracing::error!("Sync event subscriber disconnected: {e}");
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}
