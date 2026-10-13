use chrono::{Duration, Utc};
use serenity::all::*;

use database::ReviewRepository;

use super::{draft_idle_window, edit_idle_window};
use crate::framework::Data;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Draft,
    Edit,
}

impl Kind {
    fn window(self) -> Duration {
        match self {
            Kind::Draft => draft_idle_window(),
            Kind::Edit => edit_idle_window(),
        }
    }
}

pub fn arm(ctx: Context, data: Data, review_id: i64, kind: Kind) {
    tokio::spawn(async move {
        loop {
            match next_wake(&data, review_id, kind).await {
                Ok(Some(delay)) => tokio::time::sleep(delay).await,
                Ok(None) => return,
                Err(e) => {
                    tracing::error!("Review {review_id} deadline check failed: {e}");
                    return;
                }
            }
            match enforce(&ctx, &data, review_id, kind).await {
                Ok(true) => return,
                Ok(false) => continue,
                Err(e) => {
                    tracing::error!("Review {review_id} deadline enforcement failed: {e}");
                    return;
                }
            }
        }
    });
}

pub async fn hydrate(ctx: Context, data: Data) {
    match ReviewRepository::new(data.db.pool())
        .pending_deadlines()
        .await
    {
        Ok(pending) => {
            let count = pending.len();
            for (review_id, is_draft) in pending {
                let kind = if is_draft { Kind::Draft } else { Kind::Edit };
                arm(ctx.clone(), data.clone(), review_id, kind);
            }
            tracing::info!("armed {count} review deadlines");
        }
        Err(e) => tracing::error!("Failed to hydrate review deadlines: {e}"),
    }
}

pub fn after_edit_lapse(ctx: &Context, data: &Data, review_id: i64) {
    let (ctx, data) = (ctx.clone(), data.clone());
    tokio::spawn(async move {
        super::publish::rerender_post(&ctx, &data, review_id).await;
    });
}

async fn next_wake(
    data: &Data,
    review_id: i64,
    kind: Kind,
) -> anyhow::Result<Option<std::time::Duration>> {
    let revision = ReviewRepository::new(data.db.pool())
        .working_revision(review_id)
        .await?;
    let Some(revision) = revision else {
        return Ok(None);
    };
    let due = revision.last_activity_at + kind.window();
    Ok(Some((due - Utc::now()).to_std().unwrap_or_default()))
}

async fn enforce(ctx: &Context, data: &Data, review_id: i64, kind: Kind) -> anyhow::Result<bool> {
    let repo = ReviewRepository::new(data.db.pool());
    match kind {
        Kind::Draft => Ok(repo.expire_idle_draft(review_id, kind.window()).await?),
        Kind::Edit => {
            let lapsed = repo.cancel_idle_edit(review_id, kind.window()).await?;
            if lapsed {
                super::publish::rerender_post(ctx, data, review_id).await;
            }
            Ok(lapsed)
        }
    }
}
