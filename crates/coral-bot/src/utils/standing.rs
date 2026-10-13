use coral_redis::SyncEvent;
use database::{MemberRepository, standing, standing::StandingChange};

use crate::framework::Data;

pub async fn refresh_and_sync(data: &Data, discord_id: u64) -> StandingChange {
    let repo = MemberRepository::new(data.db.pool());
    let change = standing::refresh(&repo, discord_id as i64).await;
    publish_change(data, discord_id, change).await
}

pub async fn refresh_fresh_and_sync(data: &Data, discord_id: u64) -> StandingChange {
    let repo = MemberRepository::new(data.db.pool());
    let change = standing::refresh_fresh(&repo, discord_id as i64).await;
    publish_change(data, discord_id, change).await
}

async fn publish_change(
    data: &Data,
    discord_id: u64,
    change: Result<StandingChange, impl std::fmt::Display>,
) -> StandingChange {
    match change {
        Ok(change) => {
            if change.tag.is_some() {
                data.sync_event_publisher
                    .publish(&SyncEvent::SyncUser { discord_id })
                    .await;
            }
            change
        }
        Err(e) => {
            tracing::error!("Failed to refresh standing for {discord_id}: {e}");
            StandingChange::default()
        }
    }
}
