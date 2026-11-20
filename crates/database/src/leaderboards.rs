use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Leaderboard {
    Taggers,
    Requesters,
    Voters,
    AcceptedReviews,
}

impl Leaderboard {
    pub const ALL: [Self; 4] = [
        Self::Taggers,
        Self::Requesters,
        Self::Voters,
        Self::AcceptedReviews,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Taggers => "taggers",
            Self::Requesters => "requesters",
            Self::Voters => "voters",
            Self::AcceptedReviews => "accepted",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|board| board.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Taggers => "Top Taggers",
            Self::Requesters => "Top Requesters",
            Self::Voters => "Top Voters",
            Self::AcceptedReviews => "Most Accepted Reviews",
        }
    }

    /// Request counts are a lifetime counter on `members`, so they can't be windowed.
    pub fn supports_periods(self) -> bool {
        self != Self::Requesters
    }

    // `$1` is the start of the window (the Unix epoch for all-time). Requesters only
    // references it so Postgres can infer its type.
    fn ranking_sql(self) -> &'static str {
        match self {
            Self::Taggers => {
                "SELECT author, COUNT(*) FROM player_events
                 WHERE kind = 'tag_set' AND author IS NOT NULL AND author <> 0 AND ts >= $1
                 GROUP BY author"
            }
            Self::Requesters => {
                "SELECT discord_id, request_count::bigint FROM members
                 WHERE request_count > 0 AND $1::timestamptz IS NOT NULL"
            }
            Self::Voters => {
                "SELECT voter_id, COUNT(DISTINCT review_id) FROM review_vote
                 WHERE disposition <> 'voided' AND cast_at >= $1
                 GROUP BY voter_id"
            }
            Self::AcceptedReviews => {
                "SELECT submitter_id, COUNT(*) FROM review
                 WHERE status = 'accepted' AND COALESCE(resolved_at, created_at) >= $1
                 GROUP BY submitter_id"
            }
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct LeaderboardRow {
    pub discord_id: i64,
    pub total: i64,
    pub username: Option<String>,
}

pub struct LeaderboardRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> LeaderboardRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn top(
        &self,
        board: Leaderboard,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, sqlx::Error> {
        let sql = format!(
            "WITH ranked (discord_id, total) AS ({} ORDER BY 2 DESC LIMIT $2)
             SELECT r.discord_id, r.total, c.username
             FROM ranked r LEFT JOIN discord_username_cache c USING (discord_id)
             ORDER BY r.total DESC",
            board.ranking_sql()
        );
        sqlx::query_as(&sql)
            .bind(since.unwrap_or(DateTime::UNIX_EPOCH))
            .bind(limit)
            .fetch_all(self.pool)
            .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        PgPool::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()
    }

    #[tokio::test]
    async fn a_period_only_counts_tags_placed_inside_it() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let author = 930_000_000_000_000_000 + Utc::now().timestamp_subsec_nanos() as i64;
        for days_ago in [1, 2, 40] {
            sqlx::query(
                "INSERT INTO player_events (uuid, kind, tag_type, reason, author, ts)
                 VALUES ($1, 'tag_set', 'sniper', 'r', $2, NOW() - make_interval(days => $3))",
            )
            .bind(format!("lb{author:x}{days_ago}"))
            .bind(author)
            .bind(days_ago)
            .execute(&pool)
            .await
            .unwrap();
        }

        let repo = LeaderboardRepository::new(&pool);
        let count_for = |rows: Vec<LeaderboardRow>| {
            rows.into_iter()
                .find(|row| row.discord_id == author)
                .map(|row| row.total)
        };
        let weekly = repo
            .top(
                Leaderboard::Taggers,
                Some(Utc::now() - Duration::days(7)),
                1000,
            )
            .await
            .unwrap();
        let all_time = repo.top(Leaderboard::Taggers, None, 1000).await.unwrap();

        assert_eq!(count_for(weekly), Some(2));
        assert_eq!(count_for(all_time), Some(3));
    }

    #[tokio::test]
    async fn every_board_query_runs() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = LeaderboardRepository::new(&pool);
        for board in Leaderboard::ALL {
            repo.top(board, None, 10).await.unwrap();
            repo.top(board, Some(Utc::now()), 10).await.unwrap();
        }
    }
}
