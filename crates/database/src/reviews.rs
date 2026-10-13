use chrono::{DateTime, Duration, Utc};
use sqlx::{FromRow, PgConnection, PgPool};

const REVIEW_COLUMNS: &str = "id, player_uuid, submitter_id, hide_username, thread_id, status, \
    staff_resolved, resolved_by, resolved_at, voided_by, voided_at, created_at";
const REVISION_COLUMNS: &str =
    "review_id, revision, reason, tag_type, created_at, last_activity_at, published_at";
const EVIDENCE_COLUMNS: &str = "id, review_id, revision, kind, replay_id, replay_ts, filename, \
    content_type, attachment_message_id, attachment_id, note, added_by, added_at, position";
const VOTE_COLUMNS: &str = "id, review_id, voter_id, revision, kind, reason, disposition, cast_at";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    Draft,
    Open,
    Accepted,
    Rejected,
    Voided,
    Expired,
}

impl ReviewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Voided => "voided",
            Self::Expired => "expired",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "open" => Self::Open,
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            "voided" => Self::Voided,
            "expired" => Self::Expired,
            _ => Self::Draft,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Accepted | Self::Rejected | Self::Voided | Self::Expired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteKind {
    Accept,
    Reject,
    Revise,
}

impl VoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
            Self::Revise => "revise",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "reject" => Self::Reject,
            "revise" => Self::Revise,
            _ => Self::Accept,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    Replay,
    Media,
}

#[derive(Debug, Clone, FromRow)]
struct ReviewRow {
    id: i64,
    player_uuid: String,
    submitter_id: i64,
    hide_username: bool,
    thread_id: Option<i64>,
    status: String,
    staff_resolved: bool,
    resolved_by: Option<i64>,
    resolved_at: Option<DateTime<Utc>>,
    voided_by: Option<i64>,
    voided_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Review {
    pub id: i64,
    pub player_uuid: String,
    pub submitter_id: i64,
    pub hide_username: bool,
    pub thread_id: Option<i64>,
    pub status: ReviewStatus,
    pub staff_resolved: bool,
    pub resolved_by: Option<i64>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub voided_by: Option<i64>,
    pub voided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<ReviewRow> for Review {
    fn from(row: ReviewRow) -> Self {
        Self {
            id: row.id,
            player_uuid: row.player_uuid,
            submitter_id: row.submitter_id,
            hide_username: row.hide_username,
            thread_id: row.thread_id,
            status: ReviewStatus::parse(&row.status),
            staff_resolved: row.staff_resolved,
            resolved_by: row.resolved_by,
            resolved_at: row.resolved_at,
            voided_by: row.voided_by,
            voided_at: row.voided_at,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct Revision {
    pub review_id: i64,
    pub revision: i32,
    pub reason: String,
    pub tag_type: String,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct EvidenceRow {
    id: i64,
    review_id: i64,
    revision: i32,
    kind: String,
    replay_id: Option<String>,
    replay_ts: Option<String>,
    filename: Option<String>,
    content_type: Option<String>,
    attachment_message_id: Option<i64>,
    attachment_id: Option<i64>,
    note: Option<String>,
    added_by: i64,
    added_at: DateTime<Utc>,
    position: i32,
}

#[derive(Debug, Clone)]
pub struct Evidence {
    pub id: i64,
    pub review_id: i64,
    pub revision: i32,
    pub kind: EvidenceKind,
    pub replay_id: Option<String>,
    pub replay_ts: Option<String>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub attachment_message_id: Option<i64>,
    pub attachment_id: Option<i64>,
    pub note: Option<String>,
    pub added_by: i64,
    pub added_at: DateTime<Utc>,
    pub position: i32,
}

impl From<EvidenceRow> for Evidence {
    fn from(row: EvidenceRow) -> Self {
        Self {
            id: row.id,
            review_id: row.review_id,
            revision: row.revision,
            kind: if row.kind == "media" {
                EvidenceKind::Media
            } else {
                EvidenceKind::Replay
            },
            replay_id: row.replay_id,
            replay_ts: row.replay_ts,
            filename: row.filename,
            content_type: row.content_type,
            attachment_message_id: row.attachment_message_id,
            attachment_id: row.attachment_id,
            note: row.note,
            added_by: row.added_by,
            added_at: row.added_at,
            position: row.position,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StagedMedia {
    pub evidence_id: i64,
    pub filename: String,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct PublishedAttachment {
    pub evidence_id: i64,
    pub message_id: i64,
    pub attachment_id: i64,
}

#[derive(Debug, Clone)]
pub struct ContributedMedia {
    pub filename: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct VoteRow {
    id: i64,
    review_id: i64,
    voter_id: i64,
    revision: i32,
    kind: String,
    reason: Option<String>,
    disposition: String,
    cast_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Vote {
    pub id: i64,
    pub review_id: i64,
    pub voter_id: i64,
    pub revision: i32,
    pub kind: VoteKind,
    pub reason: Option<String>,
    pub disposition: String,
    pub cast_at: DateTime<Utc>,
}

impl From<VoteRow> for Vote {
    fn from(row: VoteRow) -> Self {
        Self {
            id: row.id,
            review_id: row.review_id,
            voter_id: row.voter_id,
            revision: row.revision,
            kind: VoteKind::parse(&row.kind),
            reason: row.reason,
            disposition: row.disposition,
            cast_at: row.cast_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub accept: i64,
    pub reject: i64,
    pub revise: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastOutcome {
    Recorded,
    Switched(VoteKind),
    Unchanged,
}

#[derive(Debug)]
pub enum ReviewError {
    NotFound,
    WrongStatus(ReviewStatus),
    OpenReviewExists,
    NoEditSession,
    EditInProgress,
    NoRoom(i64),
    VotesExist,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for ReviewError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

pub struct ReviewRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> ReviewRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, id: i64) -> Result<Option<Review>, sqlx::Error> {
        let row: Option<ReviewRow> = sqlx::query_as(&format!(
            "SELECT {REVIEW_COLUMNS} FROM review WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Review::from))
    }

    pub async fn get_by_thread(&self, thread_id: i64) -> Result<Option<Review>, sqlx::Error> {
        let row: Option<ReviewRow> = sqlx::query_as(&format!(
            "SELECT {REVIEW_COLUMNS} FROM review WHERE thread_id = $1"
        ))
        .bind(thread_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Review::from))
    }

    pub async fn open_for_player(&self, player_uuid: &str) -> Result<Option<Review>, sqlx::Error> {
        let row: Option<ReviewRow> = sqlx::query_as(&format!(
            "SELECT {REVIEW_COLUMNS} FROM review WHERE player_uuid = $1 AND status = 'open'"
        ))
        .bind(player_uuid)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Review::from))
    }

    pub async fn draft_for(
        &self,
        player_uuid: &str,
        submitter_id: i64,
    ) -> Result<Option<Review>, sqlx::Error> {
        let row: Option<ReviewRow> = sqlx::query_as(&format!(
            "SELECT {REVIEW_COLUMNS} FROM review
             WHERE player_uuid = $1 AND submitter_id = $2 AND status = 'draft'"
        ))
        .bind(player_uuid)
        .bind(submitter_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Review::from))
    }

    pub async fn create_draft(
        &self,
        player_uuid: &str,
        submitter_id: i64,
        hide_username: bool,
        tag_type: &str,
        reason: &str,
    ) -> Result<Review, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let row: ReviewRow = sqlx::query_as(&format!(
            "INSERT INTO review (player_uuid, submitter_id, hide_username)
             VALUES ($1, $2, $3) RETURNING {REVIEW_COLUMNS}"
        ))
        .bind(player_uuid)
        .bind(submitter_id)
        .bind(hide_username)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO review_revision (review_id, revision, reason, tag_type)
             VALUES ($1, 1, $2, $3)",
        )
        .bind(row.id)
        .bind(reason)
        .bind(tag_type)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
    }

    pub async fn live_revision(&self, review_id: i64) -> Result<Option<Revision>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {REVISION_COLUMNS} FROM review_revision
             WHERE review_id = $1 AND published_at IS NOT NULL
             ORDER BY revision DESC LIMIT 1"
        ))
        .bind(review_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn working_revision(&self, review_id: i64) -> Result<Option<Revision>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {REVISION_COLUMNS} FROM review_revision
             WHERE review_id = $1 AND published_at IS NULL
             ORDER BY revision DESC LIMIT 1"
        ))
        .bind(review_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn revision_count(&self, review_id: i64) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM review_revision WHERE review_id = $1 AND published_at IS NOT NULL",
        )
        .bind(review_id)
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    pub async fn touch(&self, review_id: i64, revision: i32) -> Result<bool, sqlx::Error> {
        let touched = sqlx::query(
            "UPDATE review_revision SET last_activity_at = NOW()
             WHERE review_id = $1 AND revision = $2 AND published_at IS NULL",
        )
        .bind(review_id)
        .bind(revision)
        .execute(self.pool)
        .await?;
        Ok(touched.rows_affected() > 0)
    }

    pub async fn set_content(
        &self,
        review_id: i64,
        revision: i32,
        tag_type: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query(
            "UPDATE review_revision SET tag_type = $3, reason = $4, last_activity_at = NOW()
             WHERE review_id = $1 AND revision = $2 AND published_at IS NULL",
        )
        .bind(review_id)
        .bind(revision)
        .bind(tag_type)
        .bind(reason)
        .execute(self.pool)
        .await?;
        Ok(updated.rows_affected() > 0)
    }

    pub async fn evidence(
        &self,
        review_id: i64,
        revision: i32,
    ) -> Result<Vec<Evidence>, sqlx::Error> {
        let rows: Vec<EvidenceRow> = sqlx::query_as(&format!(
            "SELECT {EVIDENCE_COLUMNS} FROM review_evidence
             WHERE review_id = $1 AND revision = $2 ORDER BY position, id"
        ))
        .bind(review_id)
        .bind(revision)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Evidence::from).collect())
    }

    pub async fn evidence_item(&self, evidence_id: i64) -> Result<Option<Evidence>, sqlx::Error> {
        let row: Option<EvidenceRow> = sqlx::query_as(&format!(
            "SELECT {EVIDENCE_COLUMNS} FROM review_evidence WHERE id = $1"
        ))
        .bind(evidence_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Evidence::from))
    }

    pub async fn media_count(&self, review_id: i64, revision: i32) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM review_evidence
             WHERE review_id = $1 AND revision = $2 AND kind = 'media'",
        )
        .bind(review_id)
        .bind(revision)
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    pub async fn add_replay(
        &self,
        review_id: i64,
        revision: i32,
        replay_id: &str,
        replay_ts: Option<&str>,
        note: Option<&str>,
        added_by: i64,
    ) -> Result<Evidence, sqlx::Error> {
        let row: EvidenceRow = sqlx::query_as(&format!(
            "INSERT INTO review_evidence
                 (review_id, revision, kind, replay_id, replay_ts, note, added_by, position)
             VALUES ($1, $2, 'replay', $3, $4, $5, $6, {NEXT_POSITION})
             RETURNING {EVIDENCE_COLUMNS}"
        ))
        .bind(review_id)
        .bind(revision)
        .bind(replay_id)
        .bind(replay_ts)
        .bind(note)
        .bind(added_by)
        .fetch_one(self.pool)
        .await?;
        self.touch(review_id, revision).await?;
        Ok(row.into())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_staged_media(
        &self,
        review_id: i64,
        revision: i32,
        filename: &str,
        content_type: Option<&str>,
        bytes: &[u8],
        note: Option<&str>,
        added_by: i64,
    ) -> Result<Evidence, sqlx::Error> {
        let row: EvidenceRow = sqlx::query_as(&format!(
            "INSERT INTO review_evidence
                 (review_id, revision, kind, filename, content_type, staged_bytes, note,
                  added_by, position)
             VALUES ($1, $2, 'media', $3, $4, $5, $6, $7, {NEXT_POSITION})
             RETURNING {EVIDENCE_COLUMNS}"
        ))
        .bind(review_id)
        .bind(revision)
        .bind(filename)
        .bind(content_type)
        .bind(bytes)
        .bind(note)
        .bind(added_by)
        .fetch_one(self.pool)
        .await?;
        self.touch(review_id, revision).await?;
        Ok(row.into())
    }

    pub async fn set_note(
        &self,
        evidence_id: i64,
        note: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let updated = sqlx::query(
            "UPDATE review_evidence e SET note = $2
             FROM review_revision r
             WHERE e.id = $1 AND r.review_id = e.review_id AND r.revision = e.revision
               AND r.published_at IS NULL",
        )
        .bind(evidence_id)
        .bind(note)
        .execute(self.pool)
        .await?;
        Ok(updated.rows_affected() > 0)
    }

    pub async fn remove_evidence(&self, evidence_id: i64) -> Result<bool, sqlx::Error> {
        let removed = sqlx::query(
            "DELETE FROM review_evidence e USING review_revision r
             WHERE e.id = $1 AND r.review_id = e.review_id AND r.revision = e.revision
               AND r.published_at IS NULL",
        )
        .bind(evidence_id)
        .execute(self.pool)
        .await?;
        Ok(removed.rows_affected() > 0)
    }

    pub async fn staged_media(
        &self,
        review_id: i64,
        revision: i32,
    ) -> Result<Vec<StagedMedia>, sqlx::Error> {
        let rows: Vec<(i64, String, Option<String>, Vec<u8>)> = sqlx::query_as(
            "SELECT id, filename, content_type, staged_bytes FROM review_evidence
             WHERE review_id = $1 AND revision = $2 AND staged_bytes IS NOT NULL
             ORDER BY position, id",
        )
        .bind(review_id)
        .bind(revision)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(evidence_id, filename, content_type, bytes)| StagedMedia {
                evidence_id,
                filename,
                content_type,
                bytes,
            })
            .collect())
    }

    pub async fn discard_draft(
        &self,
        review_id: i64,
        submitter_id: i64,
    ) -> Result<bool, sqlx::Error> {
        let removed = sqlx::query(
            "DELETE FROM review WHERE id = $1 AND submitter_id = $2 AND status = 'draft'",
        )
        .bind(review_id)
        .bind(submitter_id)
        .execute(self.pool)
        .await?;
        Ok(removed.rows_affected() > 0)
    }

    pub async fn expire_idle_draft(
        &self,
        review_id: i64,
        idle: Duration,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let expired = sqlx::query(
            "UPDATE review SET status = 'expired'
             WHERE id = $1 AND status = 'draft'
               AND EXISTS (SELECT 1 FROM review_revision r
                           WHERE r.review_id = review.id AND r.revision = 1
                             AND r.last_activity_at < NOW() - $2::interval)",
        )
        .bind(review_id)
        .bind(interval(idle))
        .execute(&mut *tx)
        .await?;
        if expired.rows_affected() == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM review_evidence WHERE review_id = $1")
            .bind(review_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn claim_open(&self, review_id: i64) -> Result<Review, ReviewError> {
        let row: Option<ReviewRow> = sqlx::query_as(&format!(
            "UPDATE review SET status = 'open' WHERE id = $1 AND status = 'draft'
             RETURNING {REVIEW_COLUMNS}"
        ))
        .bind(review_id)
        .fetch_optional(self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => ReviewError::OpenReviewExists,
            _ => ReviewError::Database(e),
        })?;
        match row {
            Some(row) => Ok(row.into()),
            None => Err(self.status_error(review_id).await),
        }
    }

    pub async fn release_claim(&self, review_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE review SET status = 'draft' WHERE id = $1 AND status = 'open' AND thread_id IS NULL",
        )
        .bind(review_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn finish_publish(
        &self,
        review_id: i64,
        thread_id: i64,
        attachments: &[PublishedAttachment],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE review SET thread_id = $2 WHERE id = $1")
            .bind(review_id)
            .bind(thread_id)
            .execute(&mut *tx)
            .await?;
        publish_revision(&mut tx, review_id, 1, attachments).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn edit_session(&self, review_id: i64) -> Result<Option<Revision>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {REVISION_COLUMNS} FROM review_revision r
             WHERE r.review_id = $1 AND r.published_at IS NULL
               AND r.revision > (SELECT COALESCE(MAX(revision), 0) FROM review_revision
                                 WHERE review_id = $1 AND published_at IS NOT NULL)
               AND EXISTS (SELECT 1 FROM review WHERE id = $1 AND status = 'open')"
        ))
        .bind(review_id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn begin_edit(&self, review_id: i64) -> Result<Revision, ReviewError> {
        let mut tx = self.pool.begin().await?;
        let status = lock_status(&mut tx, review_id).await?;
        if status != ReviewStatus::Open {
            return Err(ReviewError::WrongStatus(status));
        }
        let existing: Option<Revision> = sqlx::query_as(&format!(
            "UPDATE review_revision SET last_activity_at = NOW()
             WHERE review_id = $1 AND published_at IS NULL RETURNING {REVISION_COLUMNS}"
        ))
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            tx.commit().await?;
            return Ok(existing);
        }
        let revision: Revision = sqlx::query_as(&format!(
            "INSERT INTO review_revision (review_id, revision, reason, tag_type)
             SELECT review_id, revision + 1, reason, tag_type FROM review_revision
             WHERE review_id = $1 AND published_at IS NOT NULL
             ORDER BY revision DESC LIMIT 1
             RETURNING {REVISION_COLUMNS}"
        ))
        .bind(review_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO review_evidence
                 (review_id, revision, kind, replay_id, replay_ts, filename, content_type,
                  attachment_message_id, attachment_id, note, added_by, added_at, position)
             SELECT review_id, $2, kind, replay_id, replay_ts, filename, content_type,
                    attachment_message_id, attachment_id, note, added_by, added_at, position
             FROM review_evidence WHERE review_id = $1 AND revision = $2 - 1",
        )
        .bind(review_id)
        .bind(revision.revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(revision)
    }

    pub async fn cancel_edit(&self, review_id: i64) -> Result<bool, sqlx::Error> {
        let removed = sqlx::query(
            "DELETE FROM review_revision WHERE review_id = $1 AND published_at IS NULL
               AND EXISTS (SELECT 1 FROM review WHERE id = $1 AND status = 'open')",
        )
        .bind(review_id)
        .execute(self.pool)
        .await?;
        Ok(removed.rows_affected() > 0)
    }

    pub async fn cancel_idle_edit(
        &self,
        review_id: i64,
        idle: Duration,
    ) -> Result<bool, sqlx::Error> {
        let removed = sqlx::query(
            "DELETE FROM review_revision WHERE review_id = $1 AND published_at IS NULL
               AND last_activity_at < NOW() - $2::interval
               AND EXISTS (SELECT 1 FROM review WHERE id = $1 AND status = 'open')",
        )
        .bind(review_id)
        .bind(interval(idle))
        .execute(self.pool)
        .await?;
        Ok(removed.rows_affected() > 0)
    }

    pub async fn save_edit(
        &self,
        review_id: i64,
        attachments: &[PublishedAttachment],
    ) -> Result<Revision, ReviewError> {
        let mut tx = self.pool.begin().await?;
        let status = lock_status(&mut tx, review_id).await?;
        if status != ReviewStatus::Open {
            return Err(ReviewError::WrongStatus(status));
        }
        let session: Option<Revision> = sqlx::query_as(&format!(
            "SELECT {REVISION_COLUMNS} FROM review_revision
             WHERE review_id = $1 AND published_at IS NULL"
        ))
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(session) = session else {
            return Err(ReviewError::NoEditSession);
        };
        publish_revision(&mut tx, review_id, session.revision, attachments).await?;
        sqlx::query(
            "UPDATE review_vote SET disposition = 'superseded'
             WHERE review_id = $1 AND disposition = 'active' AND kind IN ('accept', 'revise')",
        )
        .bind(review_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(session)
    }

    pub async fn add_contributed_media(
        &self,
        review_id: i64,
        items: &[ContributedMedia],
        added_by: i64,
        max_media: i64,
    ) -> Result<Vec<Evidence>, ReviewError> {
        let mut tx = self.pool.begin().await?;
        let status = lock_status(&mut tx, review_id).await?;
        if status != ReviewStatus::Open {
            return Err(ReviewError::WrongStatus(status));
        }
        let (editing,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM review_revision
                            WHERE review_id = $1 AND published_at IS NULL)",
        )
        .bind(review_id)
        .fetch_one(&mut *tx)
        .await?;
        if editing {
            return Err(ReviewError::EditInProgress);
        }
        let revision = live_revision_number(&mut tx, review_id).await?;
        let (used,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM review_evidence
             WHERE review_id = $1 AND revision = $2 AND kind = 'media'",
        )
        .bind(review_id)
        .bind(revision)
        .fetch_one(&mut *tx)
        .await?;
        if used + items.len() as i64 > max_media {
            return Err(ReviewError::NoRoom((max_media - used).max(0)));
        }
        let mut added = Vec::with_capacity(items.len());
        for item in items {
            let row: EvidenceRow = sqlx::query_as(&format!(
                "INSERT INTO review_evidence
                     (review_id, revision, kind, filename, content_type, added_by, position)
                 VALUES ($1, $2, 'media', $3, $4, $5, {NEXT_POSITION})
                 RETURNING {EVIDENCE_COLUMNS}"
            ))
            .bind(review_id)
            .bind(revision)
            .bind(&item.filename)
            .bind(&item.content_type)
            .bind(added_by)
            .fetch_one(&mut *tx)
            .await?;
            added.push(row.into());
        }
        tx.commit().await?;
        Ok(added)
    }

    pub async fn attach_contributed_media(
        &self,
        review_id: i64,
        message_id: i64,
        attachments: &[(String, i64)],
    ) -> Result<(), sqlx::Error> {
        for (filename, attachment_id) in attachments {
            sqlx::query(
                "UPDATE review_evidence SET attachment_message_id = $2, attachment_id = $4
                 WHERE review_id = $1 AND filename = $3 AND attachment_id IS NULL",
            )
            .bind(review_id)
            .bind(message_id)
            .bind(filename)
            .bind(attachment_id)
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn discard_unattached_media(
        &self,
        review_id: i64,
        filenames: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "DELETE FROM review_evidence
             WHERE review_id = $1 AND filename = ANY($2) AND attachment_id IS NULL",
        )
        .bind(review_id)
        .bind(filenames)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn active_votes(&self, review_id: i64) -> Result<Vec<Vote>, sqlx::Error> {
        let rows: Vec<VoteRow> = sqlx::query_as(&format!(
            "SELECT {VOTE_COLUMNS} FROM review_vote
             WHERE review_id = $1 AND disposition = 'active' ORDER BY cast_at, id"
        ))
        .bind(review_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Vote::from).collect())
    }

    pub async fn active_vote(
        &self,
        review_id: i64,
        voter_id: i64,
    ) -> Result<Option<Vote>, sqlx::Error> {
        let row: Option<VoteRow> = sqlx::query_as(&format!(
            "SELECT {VOTE_COLUMNS} FROM review_vote
             WHERE review_id = $1 AND voter_id = $2 AND disposition = 'active'"
        ))
        .bind(review_id)
        .bind(voter_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Vote::from))
    }

    pub async fn tally(&self, review_id: i64) -> Result<Tally, sqlx::Error> {
        let (accept, reject, revise): (i64, i64, i64) = sqlx::query_as(
            "SELECT COUNT(*) FILTER (WHERE kind = 'accept'),
                    COUNT(*) FILTER (WHERE kind = 'reject'),
                    COUNT(*) FILTER (WHERE kind = 'revise')
             FROM review_vote WHERE review_id = $1 AND disposition = 'active'",
        )
        .bind(review_id)
        .fetch_one(self.pool)
        .await?;
        Ok(Tally {
            accept,
            reject,
            revise,
        })
    }

    pub async fn ever_voted(&self, review_id: i64) -> Result<bool, sqlx::Error> {
        let (exists,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM review_vote WHERE review_id = $1)")
                .bind(review_id)
                .fetch_one(self.pool)
                .await?;
        Ok(exists)
    }

    pub async fn cast_vote(
        &self,
        review_id: i64,
        voter_id: i64,
        kind: VoteKind,
        reason: Option<&str>,
    ) -> Result<CastOutcome, ReviewError> {
        let mut tx = self.pool.begin().await?;
        let status = lock_status(&mut tx, review_id).await?;
        if status != ReviewStatus::Open {
            return Err(ReviewError::WrongStatus(status));
        }
        let revision = live_revision_number(&mut tx, review_id).await?;
        let current: Option<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT id, kind, reason FROM review_vote
             WHERE review_id = $1 AND voter_id = $2 AND disposition = 'active'",
        )
        .bind(review_id)
        .bind(voter_id)
        .fetch_optional(&mut *tx)
        .await?;
        let outcome = match current {
            None => {
                sqlx::query(
                    "INSERT INTO review_vote (review_id, voter_id, revision, kind, reason)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(review_id)
                .bind(voter_id)
                .bind(revision)
                .bind(kind.as_str())
                .bind(reason)
                .execute(&mut *tx)
                .await?;
                CastOutcome::Recorded
            }
            Some((_, held, held_reason))
                if VoteKind::parse(&held) == kind && held_reason.as_deref() == reason =>
            {
                CastOutcome::Unchanged
            }
            Some((id, held, _)) if VoteKind::parse(&held) == kind => {
                sqlx::query("UPDATE review_vote SET reason = $2 WHERE id = $1")
                    .bind(id)
                    .bind(reason)
                    .execute(&mut *tx)
                    .await?;
                CastOutcome::Recorded
            }
            Some((id, held, _)) => {
                sqlx::query(
                    "UPDATE review_vote SET kind = $2, reason = $3, revision = $4, cast_at = NOW()
                     WHERE id = $1",
                )
                .bind(id)
                .bind(kind.as_str())
                .bind(reason)
                .bind(revision)
                .execute(&mut *tx)
                .await?;
                CastOutcome::Switched(VoteKind::parse(&held))
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    pub async fn resolve(
        &self,
        review_id: i64,
        outcome: ReviewStatus,
        staff_resolved: bool,
        resolved_by: Option<i64>,
    ) -> Result<Review, ReviewError> {
        let row: Option<ReviewRow> = sqlx::query_as(&format!(
            "UPDATE review SET status = $2, staff_resolved = $3, resolved_by = $4,
                 resolved_at = NOW()
             WHERE id = $1 AND status = 'open'
             RETURNING {REVIEW_COLUMNS}"
        ))
        .bind(review_id)
        .bind(outcome.as_str())
        .bind(staff_resolved)
        .bind(resolved_by)
        .fetch_optional(self.pool)
        .await?;
        match row {
            Some(row) => Ok(row.into()),
            None => Err(self.status_error(review_id).await),
        }
    }

    pub async fn void(&self, review_id: i64, voided_by: i64) -> Result<Review, ReviewError> {
        let mut tx = self.pool.begin().await?;
        let review = void_open(&mut tx, review_id, voided_by).await?;
        tx.commit().await?;
        Ok(review)
    }

    pub async fn withdraw(&self, review_id: i64, submitter_id: i64) -> Result<Review, ReviewError> {
        let mut tx = self.pool.begin().await?;
        let status = lock_status(&mut tx, review_id).await?;
        if status != ReviewStatus::Open {
            return Err(ReviewError::WrongStatus(status));
        }
        let (voted,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM review_vote WHERE review_id = $1)")
                .bind(review_id)
                .fetch_one(&mut *tx)
                .await?;
        if voted {
            return Err(ReviewError::VotesExist);
        }
        let review = void_open(&mut tx, review_id, submitter_id).await?;
        tx.commit().await?;
        Ok(review)
    }

    pub async fn pending_deadlines(&self) -> Result<Vec<(i64, bool)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT r.id, r.status = 'draft' FROM review r
             WHERE r.status = 'draft'
                OR (r.status = 'open' AND EXISTS (SELECT 1 FROM review_revision v
                                                  WHERE v.review_id = r.id
                                                    AND v.published_at IS NULL))",
        )
        .fetch_all(self.pool)
        .await
    }

    async fn status_error(&self, review_id: i64) -> ReviewError {
        match self.get(review_id).await {
            Ok(Some(review)) => ReviewError::WrongStatus(review.status),
            Ok(None) => ReviewError::NotFound,
            Err(e) => ReviewError::Database(e),
        }
    }
}

const NEXT_POSITION: &str = "(SELECT COALESCE(MAX(position), 0) + 1 FROM review_evidence \
     WHERE review_id = $1 AND revision = $2)";

fn interval(duration: Duration) -> String {
    format!("{} seconds", duration.num_seconds())
}

async fn lock_status(conn: &mut PgConnection, review_id: i64) -> Result<ReviewStatus, ReviewError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT status FROM review WHERE id = $1 FOR UPDATE")
            .bind(review_id)
            .fetch_optional(&mut *conn)
            .await?;
    row.map(|(status,)| ReviewStatus::parse(&status))
        .ok_or(ReviewError::NotFound)
}

async fn live_revision_number(conn: &mut PgConnection, review_id: i64) -> Result<i32, sqlx::Error> {
    let (revision,): (i32,) = sqlx::query_as(
        "SELECT COALESCE(MAX(revision), 1) FROM review_revision
         WHERE review_id = $1 AND published_at IS NOT NULL",
    )
    .bind(review_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(revision)
}

async fn publish_revision(
    conn: &mut PgConnection,
    review_id: i64,
    revision: i32,
    attachments: &[PublishedAttachment],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE review_revision SET published_at = NOW() WHERE review_id = $1 AND revision = $2",
    )
    .bind(review_id)
    .bind(revision)
    .execute(&mut *conn)
    .await?;
    for attachment in attachments {
        sqlx::query(
            "UPDATE review_evidence
             SET attachment_message_id = $2, attachment_id = $3, staged_bytes = NULL
             WHERE id = $1",
        )
        .bind(attachment.evidence_id)
        .bind(attachment.message_id)
        .bind(attachment.attachment_id)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query(
        "UPDATE review_evidence SET staged_bytes = NULL WHERE review_id = $1 AND revision = $2",
    )
    .bind(review_id)
    .bind(revision)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn void_open(
    conn: &mut PgConnection,
    review_id: i64,
    voided_by: i64,
) -> Result<Review, ReviewError> {
    let status = lock_status(conn, review_id).await?;
    if status != ReviewStatus::Open {
        return Err(ReviewError::WrongStatus(status));
    }
    sqlx::query("DELETE FROM review_revision WHERE review_id = $1 AND published_at IS NULL")
        .bind(review_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "UPDATE review_vote SET disposition = 'voided' WHERE review_id = $1 AND disposition = 'active'",
    )
    .bind(review_id)
    .execute(&mut *conn)
    .await?;
    let row: ReviewRow = sqlx::query_as(&format!(
        "UPDATE review SET status = 'voided', voided_by = $2, voided_at = NOW()
         WHERE id = $1 RETURNING {REVIEW_COLUMNS}"
    ))
    .bind(review_id)
    .bind(voided_by)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransferRepository;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()
    }

    fn unique(base: i64) -> i64 {
        base + Utc::now().timestamp_nanos_opt().unwrap() % 1_000_000_000_000
    }

    fn player() -> String {
        format!("{:032x}", Utc::now().timestamp_nanos_opt().unwrap())
    }

    async fn published_review(repo: &ReviewRepository<'_>, submitter: i64) -> Review {
        let draft = repo
            .create_draft(&player(), submitter, false, "blatant_cheater", "scaffold")
            .await
            .unwrap();
        repo.add_replay(
            draft.id,
            1,
            "9f2fa87d-ed0b-471b-a2e6-cb42777beec8",
            None,
            None,
            submitter,
        )
        .await
        .unwrap();
        repo.claim_open(draft.id).await.unwrap();
        repo.finish_publish(draft.id, unique(500_000_000_000_000_000), &[])
            .await
            .unwrap();
        repo.get(draft.id).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn draft_publishes_once_and_holds_the_player_slot() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let uuid = player();
        let first = repo
            .create_draft(&uuid, unique(1_000), true, "closet_cheater", "reach")
            .await
            .unwrap();
        let second = repo
            .create_draft(
                &uuid,
                unique(2_000_000_000_000),
                false,
                "closet_cheater",
                "reach",
            )
            .await
            .unwrap();
        let media = repo
            .add_staged_media(
                first.id,
                1,
                "clip.mp4",
                Some("video/mp4"),
                b"bytes",
                None,
                1,
            )
            .await
            .unwrap();
        assert_eq!(repo.staged_media(first.id, 1).await.unwrap().len(), 1);

        repo.claim_open(first.id).await.unwrap();
        assert!(matches!(
            repo.claim_open(second.id).await,
            Err(ReviewError::OpenReviewExists)
        ));
        let attachment = PublishedAttachment {
            evidence_id: media.id,
            message_id: 11,
            attachment_id: 22,
        };
        repo.finish_publish(first.id, unique(600_000_000_000_000_000), &[attachment])
            .await
            .unwrap();

        assert!(repo.staged_media(first.id, 1).await.unwrap().is_empty());
        let published = repo.evidence(first.id, 1).await.unwrap();
        assert_eq!(published[0].attachment_id, Some(22));
        assert!(repo.live_revision(first.id).await.unwrap().is_some());
        assert_eq!(
            repo.open_for_player(&uuid).await.unwrap().unwrap().id,
            first.id
        );
    }

    #[tokio::test]
    async fn votes_record_switch_and_save_clears_only_accepts_and_revises() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let review = published_review(&repo, unique(3_000_000_000_000)).await;
        let (a, b, c) = (unique(10), unique(20), unique(30));

        assert_eq!(
            repo.cast_vote(review.id, a, VoteKind::Accept, None)
                .await
                .unwrap(),
            CastOutcome::Recorded
        );
        repo.cast_vote(review.id, b, VoteKind::Reject, None)
            .await
            .unwrap();
        repo.cast_vote(review.id, c, VoteKind::Revise, Some("type is closet"))
            .await
            .unwrap();
        assert_eq!(
            repo.cast_vote(review.id, a, VoteKind::Accept, None)
                .await
                .unwrap(),
            CastOutcome::Unchanged
        );
        assert_eq!(
            repo.tally(review.id).await.unwrap(),
            Tally {
                accept: 1,
                reject: 1,
                revise: 1
            }
        );

        let session = repo.begin_edit(review.id).await.unwrap();
        assert_eq!(session.revision, 2);
        assert_eq!(repo.evidence(review.id, 2).await.unwrap().len(), 1);
        assert_eq!(repo.begin_edit(review.id).await.unwrap().revision, 2);
        assert!(repo.edit_session(review.id).await.unwrap().is_some());

        repo.save_edit(review.id, &[]).await.unwrap();
        assert!(repo.edit_session(review.id).await.unwrap().is_none());
        assert_eq!(
            repo.tally(review.id).await.unwrap(),
            Tally {
                accept: 0,
                reject: 1,
                revise: 0
            }
        );
        assert_eq!(
            repo.live_revision(review.id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            2
        );

        assert_eq!(
            repo.cast_vote(review.id, b, VoteKind::Accept, None)
                .await
                .unwrap(),
            CastOutcome::Switched(VoteKind::Reject)
        );
        assert!(matches!(
            repo.withdraw(review.id, review.submitter_id).await,
            Err(ReviewError::VotesExist)
        ));
    }

    #[tokio::test]
    async fn void_ends_a_live_edit_and_voids_votes() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let review = published_review(&repo, unique(4_000_000_000_000)).await;
        repo.cast_vote(review.id, unique(40), VoteKind::Accept, None)
            .await
            .unwrap();
        repo.begin_edit(review.id).await.unwrap();

        let voided = repo.void(review.id, 99).await.unwrap();
        assert_eq!(voided.status, ReviewStatus::Voided);
        assert!(repo.working_revision(review.id).await.unwrap().is_none());
        assert!(repo.active_votes(review.id).await.unwrap().is_empty());
        assert!(matches!(
            repo.cast_vote(review.id, unique(41), VoteKind::Accept, None)
                .await,
            Err(ReviewError::WrongStatus(ReviewStatus::Voided))
        ));
    }

    #[tokio::test]
    async fn withdraw_works_only_before_any_vote() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let review = published_review(&repo, unique(5_000_000_000_000)).await;
        let withdrawn = repo.withdraw(review.id, review.submitter_id).await.unwrap();
        assert_eq!(withdrawn.voided_by, Some(review.submitter_id));
    }

    #[tokio::test]
    async fn contributed_media_joins_the_live_revision_within_the_cap() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let review = published_review(&repo, unique(5_500_000_000_000)).await;
        let contributor = unique(55);
        let clip = |n: usize| ContributedMedia {
            filename: format!("Hexze_{n}.mp4"),
            content_type: Some("video/mp4".into()),
        };

        assert!(matches!(
            repo.add_contributed_media(review.id, &[clip(1), clip(2)], contributor, 1)
                .await,
            Err(ReviewError::NoRoom(1))
        ));
        let added = repo
            .add_contributed_media(review.id, &[clip(1)], contributor, 9)
            .await
            .unwrap();
        assert_eq!(added[0].revision, 1);
        assert_eq!(added[0].added_by, contributor);

        repo.begin_edit(review.id).await.unwrap();
        assert!(matches!(
            repo.add_contributed_media(review.id, &[clip(2)], contributor, 9)
                .await,
            Err(ReviewError::EditInProgress)
        ));
        repo.attach_contributed_media(review.id, 77, &[("Hexze_1.mp4".into(), 88)])
            .await
            .unwrap();
        for revision in [1, 2] {
            let media = repo.evidence(review.id, revision).await.unwrap();
            assert_eq!(media[1].attachment_id, Some(88));
        }
        repo.cancel_edit(review.id).await.unwrap();

        repo.add_contributed_media(review.id, &[clip(2)], contributor, 9)
            .await
            .unwrap();
        repo.discard_unattached_media(review.id, &["Hexze_2.mp4".into()])
            .await
            .unwrap();
        assert_eq!(repo.media_count(review.id, 1).await.unwrap(), 1);

        repo.void(review.id, 1).await.unwrap();
        assert!(matches!(
            repo.add_contributed_media(review.id, &[clip(3)], contributor, 9)
                .await,
            Err(ReviewError::WrongStatus(ReviewStatus::Voided))
        ));
    }

    #[tokio::test]
    async fn idle_draft_expires_and_active_one_does_not() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let draft = repo
            .create_draft(
                &player(),
                unique(6_000_000_000_000),
                false,
                "blatant_cheater",
                "x",
            )
            .await
            .unwrap();
        repo.add_replay(
            draft.id,
            1,
            "9f2fa87d-ed0b-471b-a2e6-cb42777beec8",
            None,
            None,
            1,
        )
        .await
        .unwrap();
        assert!(
            !repo
                .expire_idle_draft(draft.id, Duration::minutes(30))
                .await
                .unwrap()
        );
        sqlx::query(
            "UPDATE review_revision SET last_activity_at = NOW() - INTERVAL '31 minutes'
             WHERE review_id = $1",
        )
        .bind(draft.id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            repo.expire_idle_draft(draft.id, Duration::minutes(30))
                .await
                .unwrap()
        );
        assert_eq!(
            repo.get(draft.id).await.unwrap().unwrap().status,
            ReviewStatus::Expired
        );
        assert!(repo.evidence(draft.id, 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn transfer_merges_review_participation_without_double_counting() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = ReviewRepository::new(&pool);
        let (source, target) = (unique(7_000_000_000_000), unique(8_000_000_000_000));
        let review = published_review(&repo, unique(9_000_000_000_000)).await;
        repo.cast_vote(review.id, source, VoteKind::Accept, None)
            .await
            .unwrap();
        repo.cast_vote(review.id, target, VoteKind::Accept, None)
            .await
            .unwrap();
        let uuid = player();
        repo.create_draft(&uuid, source, false, "blatant_cheater", "s")
            .await
            .unwrap();
        let kept = repo
            .create_draft(&uuid, target, false, "blatant_cheater", "t")
            .await
            .unwrap();

        TransferRepository::new(&pool)
            .transfer(source, target, 1)
            .await
            .unwrap();

        assert_eq!(repo.tally(review.id).await.unwrap().accept, 1);
        assert_eq!(
            repo.draft_for(&uuid, target).await.unwrap().unwrap().id,
            kept.id
        );
        assert!(repo.draft_for(&uuid, source).await.unwrap().is_none());
    }
}
