use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgConnection, PgPool, Postgres, Transaction};

pub const EVIDENCE_LOCK_NAMESPACE: i32 = 7_210_001;

const POST_COLUMNS: &str = "thread_id, uuid, origin, status, title, pending_tag_type, \
    pending_reason, pending_by, created_at";

const HOLDS_CONFIRMED: &str = "EXISTS (
    SELECT 1 FROM (
        SELECT DISTINCT ON (tag_type) kind, expires_at FROM player_events pe
        WHERE pe.uuid = evidence_post.uuid AND pe.tag_type = 'confirmed_cheater'
          AND pe.kind IN ('tag_set', 'tag_clear')
        ORDER BY tag_type, ts DESC, id DESC
    ) latest
    WHERE latest.kind = 'tag_set' AND (latest.expires_at IS NULL OR latest.expires_at > NOW()))";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Coral,
    Legacy,
}

impl Origin {
    fn as_str(self) -> &'static str {
        match self {
            Origin::Coral => "coral",
            Origin::Legacy => "legacy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostStatus {
    Pending,
    Confirmed,
    Archived,
}

impl PostStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PostStatus::Pending => "pending",
            PostStatus::Confirmed => "confirmed",
            PostStatus::Archived => "archived",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(PostStatus::Pending),
            "confirmed" => Some(PostStatus::Confirmed),
            "archived" => Some(PostStatus::Archived),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTag {
    pub tag_type: String,
    pub reason: String,
    pub by: i64,
}

#[derive(Debug, Clone, FromRow)]
struct PostRow {
    thread_id: i64,
    uuid: String,
    origin: String,
    status: Option<String>,
    title: String,
    pending_tag_type: Option<String>,
    pending_reason: Option<String>,
    pending_by: Option<i64>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EvidencePost {
    pub thread_id: i64,
    pub uuid: String,
    pub origin: Origin,
    pub status: Option<PostStatus>,
    pub title: String,
    pub pending: Option<PendingTag>,
    pub created_at: DateTime<Utc>,
}

impl From<PostRow> for EvidencePost {
    fn from(row: PostRow) -> Self {
        let pending = match (row.pending_tag_type, row.pending_reason, row.pending_by) {
            (Some(tag_type), Some(reason), Some(by)) => Some(PendingTag {
                tag_type,
                reason,
                by,
            }),
            _ => None,
        };
        Self {
            thread_id: row.thread_id,
            uuid: row.uuid,
            origin: if row.origin == "coral" {
                Origin::Coral
            } else {
                Origin::Legacy
            },
            status: row.status.as_deref().and_then(PostStatus::parse),
            title: row.title,
            pending,
            created_at: row.created_at,
        }
    }
}

impl EvidencePost {
    pub fn player_name(&self) -> &str {
        self.title.split(" | ").next().unwrap_or(&self.title).trim()
    }
}

pub struct NewPost<'a> {
    pub thread_id: i64,
    pub uuid: &'a str,
    pub origin: Origin,
    pub status: Option<PostStatus>,
    pub title: &'a str,
    pub created_at: DateTime<Utc>,
}

pub struct EvidencePostRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> EvidencePostRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, post: NewPost<'_>) -> Result<bool, sqlx::Error> {
        let inserted = sqlx::query(
            "INSERT INTO evidence_post (thread_id, uuid, origin, status, title, created_at)
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (thread_id) DO NOTHING",
        )
        .bind(post.thread_id)
        .bind(post.uuid)
        .bind(post.origin.as_str())
        .bind(post.status.map(PostStatus::as_str))
        .bind(post.title)
        .bind(post.created_at)
        .execute(self.pool)
        .await?;
        Ok(inserted.rows_affected() > 0)
    }

    pub async fn delete(&self, thread_id: i64) -> Result<bool, sqlx::Error> {
        let deleted = sqlx::query("DELETE FROM evidence_post WHERE thread_id = $1")
            .bind(thread_id)
            .execute(self.pool)
            .await?;
        Ok(deleted.rows_affected() > 0)
    }

    pub async fn delete_missing(&self, present: &[i64]) -> Result<u64, sqlx::Error> {
        let deleted = sqlx::query("DELETE FROM evidence_post WHERE NOT (thread_id = ANY($1))")
            .bind(present)
            .execute(self.pool)
            .await?;
        Ok(deleted.rows_affected())
    }

    pub async fn known_thread_ids(&self) -> Result<Vec<i64>, sqlx::Error> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT thread_id FROM evidence_post")
            .fetch_all(self.pool)
            .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn get(&self, thread_id: i64) -> Result<Option<EvidencePost>, sqlx::Error> {
        self.one(
            &format!("SELECT {POST_COLUMNS} FROM evidence_post WHERE thread_id = $1"),
            thread_id,
        )
        .await
    }

    pub async fn canonical(&self, uuid: &str) -> Result<Option<EvidencePost>, sqlx::Error> {
        let row: Option<PostRow> = sqlx::query_as(&format!(
            "SELECT {POST_COLUMNS} FROM evidence_post WHERE uuid = $1 AND origin = 'coral'
             ORDER BY created_at DESC, thread_id DESC LIMIT 1"
        ))
        .bind(uuid)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(EvidencePost::from))
    }

    pub async fn link_target(&self, uuid: &str) -> Result<Option<EvidencePost>, sqlx::Error> {
        let row: Option<PostRow> = sqlx::query_as(&format!(
            "SELECT {POST_COLUMNS} FROM evidence_post WHERE uuid = $1
             ORDER BY origin = 'coral' DESC, created_at DESC, thread_id DESC LIMIT 1"
        ))
        .bind(uuid)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(EvidencePost::from))
    }

    pub async fn legacy_for(&self, uuid: &str) -> Result<Vec<EvidencePost>, sqlx::Error> {
        let rows: Vec<PostRow> = sqlx::query_as(&format!(
            "SELECT {POST_COLUMNS} FROM evidence_post WHERE uuid = $1 AND origin = 'legacy'
             ORDER BY created_at, thread_id"
        ))
        .bind(uuid)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(EvidencePost::from).collect())
    }

    pub async fn recent_coral(&self, limit: i64) -> Result<Vec<EvidencePost>, sqlx::Error> {
        let rows: Vec<PostRow> = sqlx::query_as(&format!(
            "SELECT {POST_COLUMNS} FROM evidence_post WHERE origin = 'coral'
             ORDER BY created_at DESC, thread_id DESC LIMIT $1"
        ))
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(EvidencePost::from).collect())
    }

    pub async fn search_coral(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<EvidencePost>, sqlx::Error> {
        let needle = query.trim().to_lowercase();
        let compact = needle.replace('-', "");
        let rows: Vec<PostRow> = sqlx::query_as(&format!(
            "SELECT {POST_COLUMNS} FROM evidence_post WHERE origin = 'coral'
               AND (POSITION($1 IN LOWER(SPLIT_PART(title, ' | ', 1))) > 0
                    OR ($2 <> '' AND POSITION($2 IN uuid) > 0))
             ORDER BY created_at DESC, thread_id DESC LIMIT $3"
        ))
        .bind(&needle)
        .bind(&compact)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(EvidencePost::from).collect())
    }

    pub async fn confirmed_without_tag(&self) -> Result<Vec<EvidencePost>, sqlx::Error> {
        let rows: Vec<PostRow> = sqlx::query_as(&format!(
            "SELECT {POST_COLUMNS} FROM evidence_post
             WHERE status = 'confirmed' AND NOT {HOLDS_CONFIRMED}"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(EvidencePost::from).collect())
    }

    pub async fn begin_locked(&self, uuid: &str) -> Result<Transaction<'a, Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        lock_player(&mut tx, uuid).await?;
        Ok(tx)
    }

    async fn one(&self, sql: &str, thread_id: i64) -> Result<Option<EvidencePost>, sqlx::Error> {
        let row: Option<PostRow> = sqlx::query_as(sql)
            .bind(thread_id)
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(EvidencePost::from))
    }
}

pub async fn lock_player(conn: &mut PgConnection, uuid: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(EVIDENCE_LOCK_NAMESPACE)
        .bind(uuid)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn set_status(
    conn: &mut PgConnection,
    thread_id: i64,
    status: PostStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE evidence_post SET status = $2 WHERE thread_id = $1 AND origin = 'coral'")
        .bind(thread_id)
        .bind(status.as_str())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn set_pending(
    conn: &mut PgConnection,
    thread_id: i64,
    pending: Option<&PendingTag>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE evidence_post SET pending_tag_type = $2, pending_reason = $3, pending_by = $4
         WHERE thread_id = $1",
    )
    .bind(thread_id)
    .bind(pending.map(|p| p.tag_type.as_str()))
    .bind(pending.map(|p| p.reason.as_str()))
    .bind(pending.map(|p| p.by))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get_locked(
    conn: &mut PgConnection,
    thread_id: i64,
) -> Result<Option<EvidencePost>, sqlx::Error> {
    let row: Option<PostRow> = sqlx::query_as(&format!(
        "SELECT {POST_COLUMNS} FROM evidence_post WHERE thread_id = $1"
    ))
    .bind(thread_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(EvidencePost::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()
    }

    fn unique() -> i64 {
        400_000_000_000_000_000 + Utc::now().timestamp_nanos_opt().unwrap() % 1_000_000_000_000
    }

    #[tokio::test]
    async fn canonical_prefers_newest_coral_and_link_falls_back_to_legacy() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = EvidencePostRepository::new(&pool);
        let uuid = format!("{:032x}", unique());
        let base = unique();
        let now = Utc::now();
        repo.insert(NewPost {
            thread_id: base,
            uuid: &uuid,
            origin: Origin::Legacy,
            status: None,
            title: "Hexze | old",
            created_at: now - chrono::Duration::days(30),
        })
        .await
        .unwrap();
        assert_eq!(
            repo.link_target(&uuid).await.unwrap().unwrap().thread_id,
            base
        );
        assert!(repo.canonical(&uuid).await.unwrap().is_none());

        for (offset, days) in [(1, 10), (2, 1)] {
            repo.insert(NewPost {
                thread_id: base + offset,
                uuid: &uuid,
                origin: Origin::Coral,
                status: Some(PostStatus::Pending),
                title: "Hexze | 0000",
                created_at: now - chrono::Duration::days(days),
            })
            .await
            .unwrap();
        }
        assert_eq!(
            repo.canonical(&uuid).await.unwrap().unwrap().thread_id,
            base + 2
        );
        assert_eq!(
            repo.link_target(&uuid).await.unwrap().unwrap().thread_id,
            base + 2
        );
        assert_eq!(repo.legacy_for(&uuid).await.unwrap().len(), 1);
        assert!(
            repo.search_coral("hexz", 50)
                .await
                .unwrap()
                .iter()
                .any(|p| p.thread_id == base + 2)
        );

        let mut tx = repo.begin_locked(&uuid).await.unwrap();
        let pending = PendingTag {
            tag_type: "blatant_cheater".into(),
            reason: "scaff".into(),
            by: 9,
        };
        set_pending(&mut tx, base + 2, Some(&pending))
            .await
            .unwrap();
        set_status(&mut tx, base + 2, PostStatus::Confirmed)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let post = repo.get(base + 2).await.unwrap().unwrap();
        assert_eq!(post.pending, Some(pending));
        assert_eq!(post.status, Some(PostStatus::Confirmed));
        assert!(
            repo.confirmed_without_tag()
                .await
                .unwrap()
                .iter()
                .any(|p| p.thread_id == base + 2)
        );

        for offset in 0..3 {
            repo.delete(base + offset).await.unwrap();
        }
    }
}
