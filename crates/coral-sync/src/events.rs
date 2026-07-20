use serenity::all::*;

use coral_redis::{SyncEvent, SyncEventSubscriber};

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
                    match event {
                        SyncEvent::SyncUser { discord_id } => {
                            crate::sync::sync_user(ctx, data, UserId::new(discord_id)).await;
                        }
                        SyncEvent::GuildJobQueued { job_id } => {
                            crate::jobs::spawn_job(ctx, data, job_id);
                        }
                        SyncEvent::GuildJobCancelRequested { job_id } => {
                            crate::jobs::request_cancel(&data, job_id);
                        }
                        SyncEvent::GuildJobProgress { .. } | SyncEvent::GuildJobFinished { .. } => {
                        }
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
