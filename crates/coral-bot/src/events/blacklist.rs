use chrono::Utc;
use serenity::all::*;

use coral_redis::{BlacklistEvent, EventSubscriber};
use database::{BlacklistRepository, CacheRepository, PlayerEvent};

use crate::commands::blacklist::channel;
use crate::framework::Data;

pub fn spawn_subscriber(ctx: Context, data: Data) {
    let redis_url = data.redis_url.clone();

    tokio::spawn(async move {
        loop {
            let ctx = ctx.clone();
            let data = data.clone();

            let result = EventSubscriber::run(&redis_url, move |event| {
                let ctx = ctx.clone();
                let data = data.clone();
                async move {
                    if let Err(e) = handle_event(&ctx, &data, event).await {
                        tracing::error!("Failed to handle blacklist event: {e}");
                    }
                }
            })
            .await;

            if let Err(e) = result {
                tracing::error!("Blacklist event subscriber disconnected: {e}");
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

async fn handle_event(ctx: &Context, data: &Data, event: BlacklistEvent) -> anyhow::Result<()> {
    let repo = BlacklistRepository::new(data.db.pool());
    let cache = CacheRepository::new(data.db.pool());

    match event {
        BlacklistEvent::TagAdded {
            uuid,
            tag_id,
            silent,
            ..
        } => {
            let tag = fetch_event(&repo, tag_id, "TagAdded").await?;
            if let (Some(tag_type), Some(expires_at)) = (tag.tag_type.clone(), tag.expires_at) {
                schedule_expiry(data.clone(), uuid.clone(), tag_type, tag_id, expires_at);
            }
            let all_tags = repo.get_active_tags(&uuid).await.unwrap_or_default();
            let name = resolve_name(&cache, &uuid).await;
            channel::post_new_tag(ctx, data, &uuid, &name, &tag, &all_tags, silent).await;
        }

        BlacklistEvent::TagOverwritten {
            uuid,
            old_tag_id,
            new_tag_id,
            overwritten_by,
            silent,
            ..
        } => {
            let new_tag = fetch_event(&repo, new_tag_id, "TagOverwritten").await?;
            let old_tag = fetch_event(&repo, old_tag_id, "TagOverwritten").await?;
            let all_tags = repo.get_active_tags(&uuid).await.unwrap_or_default();
            let name = resolve_name(&cache, &uuid).await;
            channel::post_tag_changed(
                ctx,
                data,
                &uuid,
                &name,
                &old_tag,
                &new_tag,
                "Tag Overwritten",
                overwritten_by as u64,
            )
            .await;
            channel::post_overwritten_tag(ctx, data, &uuid, &name, &new_tag, &all_tags, silent)
                .await;
        }

        BlacklistEvent::TagRemoved {
            uuid,
            tag_id,
            removed_by,
            silent,
        } => {
            let Some(tag) = repo.get_event_by_id(tag_id).await? else {
                tracing::warn!("event {tag_id} not found for TagRemoved");
                return Ok(());
            };
            let name = resolve_name(&cache, &uuid).await;
            channel::post_tag_removed(ctx, data, &uuid, &name, &tag, removed_by as u64, silent)
                .await;
        }

        BlacklistEvent::PlayerLocked {
            uuid,
            locked_by,
            reason,
        } => {
            let name = resolve_name(&cache, &uuid).await;
            channel::post_lock_change(
                ctx,
                data,
                &uuid,
                &name,
                true,
                Some(&reason),
                locked_by as u64,
            )
            .await;
        }

        BlacklistEvent::PlayerUnlocked { uuid, unlocked_by } => {
            let name = resolve_name(&cache, &uuid).await;
            channel::post_lock_change(ctx, data, &uuid, &name, false, None, unlocked_by as u64)
                .await;
        }

        BlacklistEvent::TagEdited { .. } => {}
    }

    Ok(())
}

async fn fetch_event(
    repo: &BlacklistRepository<'_>,
    event_id: i64,
    event_name: &str,
) -> anyhow::Result<PlayerEvent> {
    repo.get_event_by_id(event_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("event {event_id} not found for {event_name}"))
}

async fn resolve_name(cache: &CacheRepository<'_>, uuid: &str) -> String {
    cache
        .get_username(uuid)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| uuid.to_string())
}

fn schedule_expiry(
    data: Data,
    uuid: String,
    tag_type: String,
    tag_id: i64,
    expires_at: chrono::DateTime<Utc>,
) {
    tokio::spawn(async move {
        let delay = (expires_at - Utc::now()).to_std().unwrap_or_default();
        tokio::time::sleep(delay).await;
        let repo = BlacklistRepository::new(data.db.pool());
        if repo
            .remove_event(&uuid, &tag_type, None)
            .await
            .unwrap_or(false)
        {
            data.event_publisher
                .publish(&BlacklistEvent::TagRemoved {
                    uuid,
                    tag_id,
                    removed_by: 0,
                    silent: false,
                })
                .await;
        }
    });
}

pub async fn hydrate_expiring_tags(data: Data) {
    let repo = BlacklistRepository::new(data.db.pool());
    match repo.get_active_expiring_tags().await {
        Ok(tags) => {
            let count = tags.len();
            for tag in tags {
                if let (Some(tag_type), Some(expires_at)) = (tag.tag_type, tag.expires_at) {
                    schedule_expiry(data.clone(), tag.uuid, tag_type, tag.id, expires_at);
                }
            }
            tracing::info!("scheduled expiry for {count} active tags");
        }
        Err(e) => tracing::error!("failed to hydrate expiring tags: {e}"),
    }
}
