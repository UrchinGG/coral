use anyhow::Result;
use blacklist::lookup as lookup_tag;
use coral_redis::BlacklistEvent;
use serenity::all::*;

use database::{
    AddOutcome, BlacklistRepository, MemberRepository, Review, ReviewError, ReviewRepository,
    ReviewRules, ReviewStatus, Tally, Vote, VoteKind,
};

use super::publish::{rerender_post, set_forum_tags, thread_link};
use crate::framework::Data;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Accept,
    Reject,
}

impl Outcome {
    fn status(self) -> ReviewStatus {
        match self {
            Outcome::Accept => ReviewStatus::Accepted,
            Outcome::Reject => ReviewStatus::Rejected,
        }
    }

    fn winning_kind(self) -> VoteKind {
        match self {
            Outcome::Accept => VoteKind::Accept,
            Outcome::Reject => VoteKind::Reject,
        }
    }

    fn losing_kind(self) -> VoteKind {
        match self {
            Outcome::Accept => VoteKind::Reject,
            Outcome::Reject => VoteKind::Accept,
        }
    }
}

pub fn community_outcome(tally: Tally, rules: &ReviewRules) -> Option<Outcome> {
    if tally.accept >= rules.accept_threshold && tally.reject == 0 && tally.revise == 0 {
        Some(Outcome::Accept)
    } else if tally.reject >= rules.reject_threshold && tally.accept == 0 {
        Some(Outcome::Reject)
    } else {
        None
    }
}

pub fn is_contested(tally: Tally) -> bool {
    tally.accept > 0 && tally.reject > 0
}

pub async fn resolve(
    ctx: &Context,
    data: &Data,
    review_id: i64,
    outcome: Outcome,
    staff: Option<u64>,
) -> Result<bool> {
    let repo = ReviewRepository::new(data.db.pool());
    let votes = repo.active_votes(review_id).await?;
    let review = match repo
        .resolve(
            review_id,
            outcome.status(),
            staff.is_some(),
            staff.map(|id| id as i64),
        )
        .await
    {
        Ok(review) => review,
        Err(ReviewError::WrongStatus(_)) => return Ok(false),
        Err(e) => return Err(anyhow::anyhow!("{e:?}")),
    };
    score(data, &review, &votes, outcome).await;
    let mut awaiting_confirmation = false;
    if outcome == Outcome::Accept {
        apply_tag(data, &review, &votes, staff).await?;
        awaiting_confirmation = super::staff_ping::ping_if_media(ctx, data, &review).await;
    }
    finish(ctx, data, &review, !awaiting_confirmation).await;
    Ok(true)
}

pub async fn close_out(ctx: &Context, data: &Data, review: &Review) {
    finish(ctx, data, review, true).await;
}

async fn finish(ctx: &Context, data: &Data, review: &Review, close_thread: bool) {
    rerender_post(ctx, data, review.id).await;
    let Some(thread_id) = review.thread_id else {
        return;
    };
    set_forum_tags(ctx, data, thread_id, review.status).await;
    if !close_thread {
        return;
    }
    if let Err(e) = ThreadId::new(thread_id as u64)
        .edit(&ctx.http, EditThread::new().archived(true).locked(true))
        .await
    {
        tracing::warn!("Failed to close review thread {thread_id}: {e}");
    }
}

async fn score(data: &Data, review: &Review, votes: &[Vote], outcome: Outcome) {
    let repo = MemberRepository::new(data.db.pool());
    let submitter = review.submitter_id;
    let submitter_result = match outcome {
        Outcome::Accept => repo.increment_accepted_tags(submitter).await,
        Outcome::Reject => repo.increment_rejected_tags(submitter).await,
    };
    if let Err(e) = submitter_result {
        tracing::error!(
            "Failed to score submitter {submitter} on review {}: {e}",
            review.id
        );
    }

    let voters = |kind: VoteKind| -> Vec<i64> {
        votes
            .iter()
            .filter(|v| v.kind == kind)
            .map(|v| v.voter_id)
            .collect()
    };
    let winners = voters(outcome.winning_kind());
    let losers = voters(outcome.losing_kind());
    if let Err(e) = repo.increment_accurate_verdicts(&winners).await {
        tracing::error!(
            "Failed to score accurate verdicts on review {}: {e}",
            review.id
        );
    }
    if let Err(e) = repo.increment_incorrect_verdicts(&losers).await {
        tracing::error!(
            "Failed to score incorrect verdicts on review {}: {e}",
            review.id
        );
    }
    if review.staff_resolved
        && !winners.is_empty()
        && !losers.is_empty()
        && let Err(e) = repo.increment_bonus_verdicts(&winners).await
    {
        tracing::error!(
            "Failed to score bonus verdicts on review {}: {e}",
            review.id
        );
    }

    for id in std::iter::once(submitter).chain(winners).chain(losers) {
        crate::utils::standing::refresh_and_sync(data, id as u64).await;
    }
}

async fn apply_tag(data: &Data, review: &Review, votes: &[Vote], staff: Option<u64>) -> Result<()> {
    let review_repo = ReviewRepository::new(data.db.pool());
    let Some(revision) = review_repo.live_revision(review.id).await? else {
        return Ok(());
    };
    let tag_type = revision.tag_type.as_str();
    let repo = BlacklistRepository::new(data.db.pool());
    let priority = lookup_tag(tag_type).map_or(0, |d| d.priority);
    let actor = staff.map(|id| id as i64).unwrap_or(review.submitter_id);
    for existing in repo.get_active_tags(&review.player_uuid).await? {
        let existing_type = existing.tag_type.as_deref().unwrap_or_default();
        let same_group = lookup_tag(existing_type).map_or(0, |d| d.priority) == priority;
        if existing_type != tag_type && same_group {
            repo.remove_event(&review.player_uuid, existing_type, Some(actor))
                .await?;
        }
    }

    let reviewed_by: Vec<i64> = match staff {
        Some(staff) => vec![staff as i64],
        None => votes
            .iter()
            .filter(|v| v.kind == VoteKind::Accept)
            .map(|v| v.voter_id)
            .collect(),
    };
    let reviewed_by = (!reviewed_by.is_empty()).then_some(reviewed_by.as_slice());
    let blocking = vec![tag_type.to_string()];
    match repo
        .add_event(
            &review.player_uuid,
            tag_type,
            &revision.reason,
            review.hide_username,
            None,
            reviewed_by,
            Some(review.submitter_id),
            &blocking,
        )
        .await?
    {
        AddOutcome::Inserted(tag_id) => {
            data.event_publisher
                .publish(&BlacklistEvent::TagAdded {
                    uuid: review.player_uuid.clone(),
                    tag_id,
                    added_by: review.submitter_id,
                    silent: false,
                    review_url: thread_link(data, review.thread_id),
                })
                .await;
        }
        AddOutcome::Conflict(_) => {
            tracing::warn!(
                "Accepted review {} found {tag_type} already present on {}",
                review.id,
                review.player_uuid
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(accept: i64, reject: i64, revise: i64) -> Tally {
        Tally {
            accept,
            reject,
            revise,
        }
    }

    fn outcome(accept: i64, reject: i64, revise: i64) -> Option<Outcome> {
        community_outcome(tally(accept, reject, revise), &ReviewRules::DEFAULT)
    }

    #[test]
    fn eight_clean_accepts_resolve() {
        assert_eq!(outcome(7, 0, 0), None);
        assert_eq!(outcome(8, 0, 0), Some(Outcome::Accept));
    }

    #[test]
    fn edited_thresholds_apply() {
        let rules = ReviewRules {
            accept_threshold: 4,
            reject_threshold: 5,
            ..ReviewRules::DEFAULT
        };
        assert_eq!(
            community_outcome(tally(4, 0, 0), &rules),
            Some(Outcome::Accept)
        );
        assert_eq!(community_outcome(tally(0, 4, 0), &rules), None);
    }

    #[test]
    fn any_reject_or_revise_blocks_acceptance() {
        assert_eq!(outcome(9, 1, 0), None);
        assert_eq!(outcome(9, 0, 1), None);
    }

    #[test]
    fn three_rejects_with_no_accepts_reject_and_revises_do_not_count() {
        assert_eq!(outcome(0, 3, 0), Some(Outcome::Reject));
        assert_eq!(outcome(0, 2, 5), None);
        assert_eq!(outcome(0, 3, 4), Some(Outcome::Reject));
        assert_eq!(outcome(1, 3, 0), None);
    }
}
