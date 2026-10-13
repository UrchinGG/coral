use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::{FromRow, PgConnection, PgPool};

pub const IMPORTED_STRIKE_DAYS: i64 = 30;

const STRIKE_COLUMNS: &str = "s.id, s.member_id, m.discord_id, s.reason, s.struck_by, s.issued_at, \
    s.expires_at, s.revoked_at, s.revoked_by, s.source_ref";

#[derive(Debug, Clone, FromRow)]
pub struct Strike {
    pub id: i64,
    pub member_id: i64,
    pub discord_id: i64,
    pub reason: String,
    pub struck_by: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<i64>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrikeStatus {
    Active,
    Expired,
    Revoked,
}

impl Strike {
    pub fn status_at(&self, now: DateTime<Utc>) -> StrikeStatus {
        if self.revoked_at.is_some() {
            StrikeStatus::Revoked
        } else if self.expires_at.is_some_and(|at| at <= now) {
            StrikeStatus::Expired
        } else {
            StrikeStatus::Active
        }
    }

    pub fn is_active(&self) -> bool {
        self.status_at(Utc::now()) == StrikeStatus::Active
    }
}

pub struct StrikeRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> StrikeRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn issue(
        &self,
        discord_id: i64,
        reason: &str,
        struck_by: i64,
        expires_at: Option<DateTime<Utc>>,
        source_ref: Option<&str>,
    ) -> Result<Option<Strike>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let Some(member_id) = lock_member(&mut tx, discord_id).await? else {
            return Ok(None);
        };
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO member_strike (member_id, reason, struck_by, expires_at, source_ref)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(member_id)
        .bind(reason)
        .bind(struck_by)
        .bind(expires_at)
        .bind(source_ref)
        .fetch_one(&mut *tx)
        .await?;
        let strike = fetch(&mut tx, id).await?;
        sqlx::query("UPDATE members SET strikes = strikes || $2::jsonb WHERE id = $1")
            .bind(member_id)
            .bind(json!([mirror_entry(&strike)]))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(strike))
    }

    pub async fn revoke(&self, id: i64, revoked_by: i64) -> Result<Option<Strike>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let revoked = sqlx::query(
            "UPDATE member_strike SET revoked_at = NOW(), revoked_by = $2
             WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(revoked_by)
        .execute(&mut *tx)
        .await?;
        if revoked.rows_affected() == 0 {
            return Ok(None);
        }
        let strike = fetch(&mut tx, id).await?;
        drop_mirror_entry(&mut tx, &strike).await?;
        tx.commit().await?;
        Ok(Some(strike))
    }

    pub async fn settle_expiry(&self, id: i64) -> Result<Option<Strike>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let strike = fetch(&mut tx, id).await?;
        if strike.status_at(Utc::now()) != StrikeStatus::Expired {
            return Ok(None);
        }
        if !drop_mirror_entry(&mut tx, &strike).await? {
            return Ok(None);
        }
        tx.commit().await?;
        Ok(Some(strike))
    }

    pub async fn get(&self, id: i64) -> Result<Option<Strike>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {STRIKE_COLUMNS} FROM member_strike s JOIN members m ON m.id = s.member_id
             WHERE s.id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn list_for_member(&self, discord_id: i64) -> Result<Vec<Strike>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {STRIKE_COLUMNS} FROM member_strike s JOIN members m ON m.id = s.member_id
             WHERE m.discord_id = $1 ORDER BY s.issued_at DESC, s.id DESC"
        ))
        .bind(discord_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn with_pending_expiry(&self) -> Result<Vec<Strike>, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {STRIKE_COLUMNS} FROM member_strike s JOIN members m ON m.id = s.member_id
             WHERE s.revoked_at IS NULL AND s.expires_at IS NOT NULL
               AND (s.expires_at > NOW()
                    OR m.strikes @> jsonb_build_array(jsonb_build_object('strike_id', s.id)))"
        ))
        .fetch_all(self.pool)
        .await
    }

    pub async fn import_unrecorded(&self) -> Result<u64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let members: Vec<(i64, Value)> = sqlx::query_as(
            "SELECT id, strikes FROM members
             WHERE EXISTS (SELECT 1 FROM jsonb_array_elements(strikes) e WHERE NOT e ? 'strike_id')
             FOR UPDATE",
        )
        .fetch_all(&mut *tx)
        .await?;
        let expires_at = Utc::now() + Duration::days(IMPORTED_STRIKE_DAYS);
        let mut imported = 0;
        for (member_id, strikes) in members {
            let mut mirrored = Vec::new();
            for entry in strikes.as_array().cloned().unwrap_or_default() {
                if entry.get("strike_id").is_some() {
                    mirrored.push(entry);
                    continue;
                }
                let (id,): (i64,) = sqlx::query_as(
                    "INSERT INTO member_strike (member_id, reason, struck_by, issued_at, expires_at)
                     VALUES ($1, $2, $3, $4, $5) RETURNING id",
                )
                .bind(member_id)
                .bind(entry.get("reason").and_then(Value::as_str).unwrap_or("No reason provided"))
                .bind(entry.get("struck_by").and_then(Value::as_i64).unwrap_or(0))
                .bind(legacy_issued_at(&entry))
                .bind(expires_at)
                .fetch_one(&mut *tx)
                .await?;
                let mut entry = entry;
                entry["strike_id"] = json!(id);
                mirrored.push(entry);
                imported += 1;
            }
            sqlx::query("UPDATE members SET strikes = $2 WHERE id = $1")
                .bind(member_id)
                .bind(Value::Array(mirrored))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(imported)
    }
}

async fn lock_member(conn: &mut PgConnection, discord_id: i64) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM members WHERE discord_id = $1 FOR UPDATE")
            .bind(discord_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.map(|(id,)| id))
}

async fn fetch(conn: &mut PgConnection, id: i64) -> Result<Strike, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {STRIKE_COLUMNS} FROM member_strike s JOIN members m ON m.id = s.member_id
         WHERE s.id = $1"
    ))
    .bind(id)
    .fetch_one(&mut *conn)
    .await
}

async fn drop_mirror_entry(conn: &mut PgConnection, strike: &Strike) -> Result<bool, sqlx::Error> {
    let dropped = sqlx::query(
        "UPDATE members SET strikes = COALESCE((
             SELECT jsonb_agg(e ORDER BY ord)
             FROM jsonb_array_elements(strikes) WITH ORDINALITY AS t(e, ord)
             WHERE (e ->> 'strike_id')::bigint IS DISTINCT FROM $2), '[]'::jsonb)
         WHERE id = $1 AND strikes @> jsonb_build_array(jsonb_build_object('strike_id', $2::bigint))",
    )
    .bind(strike.member_id)
    .bind(strike.id)
    .execute(&mut *conn)
    .await?;
    Ok(dropped.rows_affected() > 0)
}

fn mirror_entry(strike: &Strike) -> Value {
    json!({
        "reason": strike.reason,
        "struck_by": strike.struck_by,
        "timestamp": strike.issued_at.to_rfc3339(),
        "strike_id": strike.id,
    })
}

fn legacy_issued_at(entry: &Value) -> DateTime<Utc> {
    entry
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map_or_else(Utc::now, |at| at.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strike(expires_at: Option<DateTime<Utc>>, revoked: bool) -> Strike {
        Strike {
            id: 1,
            member_id: 1,
            discord_id: 1,
            reason: "r".into(),
            struck_by: 2,
            issued_at: Utc::now(),
            expires_at,
            revoked_at: revoked.then(Utc::now),
            revoked_by: None,
            source_ref: None,
        }
    }

    #[test]
    fn status_follows_revocation_then_expiry() {
        let now = Utc::now();
        assert_eq!(strike(None, false).status_at(now), StrikeStatus::Active);
        assert_eq!(
            strike(Some(now + Duration::days(1)), false).status_at(now),
            StrikeStatus::Active
        );
        assert_eq!(
            strike(Some(now - Duration::days(1)), false).status_at(now),
            StrikeStatus::Expired
        );
        assert_eq!(
            strike(Some(now - Duration::days(1)), true).status_at(now),
            StrikeStatus::Revoked
        );
    }

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()
    }

    async fn member_with_legacy_strike(pool: &PgPool) -> i64 {
        let discord_id =
            800_000_000_000_000_000 + Utc::now().timestamp_nanos_opt().unwrap() % 1_000_000_000_000;
        sqlx::query("INSERT INTO members (discord_id, strikes) VALUES ($1, $2)")
            .bind(discord_id)
            .bind(json!([{"reason": "old", "struck_by": 7, "timestamp": "2026-01-01T00:00:00Z"}]))
            .execute(pool)
            .await
            .unwrap();
        discord_id
    }

    async fn mirror(pool: &PgPool, discord_id: i64) -> Vec<Value> {
        let (strikes,): (Value,) =
            sqlx::query_as("SELECT strikes FROM members WHERE discord_id = $1")
                .bind(discord_id)
                .fetch_one(pool)
                .await
                .unwrap();
        strikes.as_array().cloned().unwrap_or_default()
    }

    async fn active_count(pool: &PgPool, discord_id: i64) -> i64 {
        crate::MemberRepository::new(pool)
            .get_by_discord_id(discord_id)
            .await
            .unwrap()
            .unwrap()
            .active_strikes
    }

    #[tokio::test]
    async fn import_issue_revoke_and_expire_keep_the_mirror_in_step() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let discord_id = member_with_legacy_strike(&pool).await;
        let repo = StrikeRepository::new(&pool);

        assert!(repo.import_unrecorded().await.unwrap() >= 1);
        assert_eq!(repo.import_unrecorded().await.unwrap(), 0);
        let imported = repo.list_for_member(discord_id).await.unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(
            imported[0].issued_at.to_rfc3339(),
            "2026-01-01T00:00:00+00:00"
        );
        assert!(imported[0].expires_at.is_some());
        assert_eq!(
            mirror(&pool, discord_id).await[0]["strike_id"],
            json!(imported[0].id)
        );

        let issued = repo
            .issue(discord_id, "new", 9, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active_count(&pool, discord_id).await, 2);
        assert_eq!(mirror(&pool, discord_id).await.len(), 2);

        assert!(repo.revoke(issued.id, 9).await.unwrap().is_some());
        assert!(repo.revoke(issued.id, 9).await.unwrap().is_none());
        assert_eq!(active_count(&pool, discord_id).await, 1);
        assert_eq!(mirror(&pool, discord_id).await.len(), 1);

        sqlx::query(
            "UPDATE member_strike SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1",
        )
        .bind(imported[0].id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(active_count(&pool, discord_id).await, 0);
        assert!(
            repo.with_pending_expiry()
                .await
                .unwrap()
                .iter()
                .any(|s| s.id == imported[0].id)
        );
        assert!(repo.settle_expiry(imported[0].id).await.unwrap().is_some());
        assert!(repo.settle_expiry(imported[0].id).await.unwrap().is_none());
        assert!(mirror(&pool, discord_id).await.is_empty());

        sqlx::query("DELETE FROM members WHERE discord_id = $1")
            .bind(discord_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn issuing_to_an_unregistered_member_does_nothing() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let missing =
            700_000_000_000_000_000 + Utc::now().timestamp_nanos_opt().unwrap() % 1_000_000;
        assert!(
            StrikeRepository::new(&pool)
                .issue(missing, "r", 1, None, None)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn legacy_timestamp_parses_rfc3339() {
        let entry = json!({"timestamp": "2026-01-02T03:04:05Z"});
        assert_eq!(
            legacy_issued_at(&entry).to_rfc3339(),
            "2026-01-02T03:04:05+00:00"
        );
    }
}
