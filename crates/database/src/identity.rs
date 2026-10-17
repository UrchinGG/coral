use sqlx::{PgConnection, PgPool};

const HOLDER: &str = "
WITH owners AS (SELECT id, discord_id FROM members WHERE uuid = $1)
SELECT o.id, o.discord_id FROM owners o
JOIN mc_identity_claims c ON c.uuid = $1 AND c.member_id = o.id
UNION ALL
SELECT id, discord_id FROM owners WHERE (SELECT COUNT(*) FROM owners) = 1
LIMIT 1";

pub struct IdentityRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> IdentityRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn holder(&self, uuid: &str) -> Result<Option<i64>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Ok(holder(&mut conn, uuid)
            .await?
            .map(|(_, discord_id)| discord_id))
    }

    pub async fn claim(&self, member_id: i64, uuid: &str) -> Result<Option<i64>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(uuid)
            .execute(&mut *tx)
            .await?;
        let previous = holder(&mut tx, uuid).await?;
        sqlx::query(
            "INSERT INTO mc_identity_claims (uuid, member_id) VALUES ($1, $2)
             ON CONFLICT (uuid) DO UPDATE SET member_id = EXCLUDED.member_id, claimed_at = now()",
        )
        .bind(uuid)
        .bind(member_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(previous
            .filter(|(id, _)| *id != member_id)
            .map(|(_, discord_id)| discord_id))
    }
}

async fn holder(conn: &mut PgConnection, uuid: &str) -> Result<Option<(i64, i64)>, sqlx::Error> {
    sqlx::query_as(HOLDER).bind(uuid).fetch_optional(conn).await
}
