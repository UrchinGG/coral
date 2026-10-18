use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgConnection, PgPool};

use crate::history::History;

#[derive(Debug, Clone, FromRow)]
pub struct MinecraftAccount {
    pub id: i64,
    pub member_id: i64,
    pub uuid: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRole {
    Primary,
    Alt,
}

pub struct AccountRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> AccountRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn add(&self, member_id: i64, uuid: &str) -> Result<MinecraftAccount, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let account = sqlx::query_as(
            "INSERT INTO minecraft_accounts (member_id, uuid)
             VALUES ($1, $2)
             ON CONFLICT (member_id, uuid) DO UPDATE SET member_id = EXCLUDED.member_id
             RETURNING id, member_id, uuid, added_at",
        )
        .bind(member_id)
        .bind(uuid)
        .fetch_one(&mut *tx)
        .await?;
        History::Links.sync(&mut tx, member_id).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn claim(
        &self,
        member_id: i64,
        uuid: &str,
        role: LinkRole,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext('mc_link:' || $1))")
            .bind(uuid)
            .execute(&mut *tx)
            .await?;
        let mut losers = take_primary(&mut tx, member_id, uuid).await?;
        losers.extend(take_alts(&mut tx, member_id, uuid).await?);
        match role {
            LinkRole::Primary => {
                sqlx::query("UPDATE members SET uuid = $2 WHERE id = $1")
                    .bind(member_id)
                    .bind(uuid)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM minecraft_accounts WHERE member_id = $1 AND uuid = $2")
                    .bind(member_id)
                    .bind(uuid)
                    .execute(&mut *tx)
                    .await?;
            }
            LinkRole::Alt => {
                sqlx::query(
                    "INSERT INTO minecraft_accounts (member_id, uuid) VALUES ($1, $2)
                     ON CONFLICT (member_id, uuid) DO NOTHING",
                )
                .bind(member_id)
                .bind(uuid)
                .execute(&mut *tx)
                .await?;
            }
        }
        losers.sort_unstable();
        losers.dedup();
        for &(id, _) in &losers {
            History::Links.sync(&mut tx, id).await?;
        }
        History::Links.sync(&mut tx, member_id).await?;
        tx.commit().await?;
        Ok(losers
            .into_iter()
            .map(|(_, discord_id)| discord_id)
            .collect())
    }

    pub async fn remove(&self, member_id: i64, uuid: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let removed =
            sqlx::query("DELETE FROM minecraft_accounts WHERE member_id = $1 AND uuid = $2")
                .bind(member_id)
                .bind(uuid)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;
        History::Links.sync(&mut tx, member_id).await?;
        tx.commit().await?;
        Ok(removed)
    }

    pub async fn list(&self, member_id: i64) -> Result<Vec<MinecraftAccount>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, member_id, uuid, added_at
             FROM minecraft_accounts
             WHERE member_id = $1
             ORDER BY added_at",
        )
        .bind(member_id)
        .fetch_all(self.pool)
        .await
    }

    pub async fn list_uuids(&self, discord_id: i64) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT uuid FROM members WHERE discord_id = $1 AND uuid IS NOT NULL
             UNION
             SELECT ma.uuid FROM minecraft_accounts ma
             JOIN members m ON m.id = ma.member_id
             WHERE m.discord_id = $1",
        )
        .bind(discord_id)
        .fetch_all(self.pool)
        .await
    }

    /// Discord IDs of every member who has this account linked.
    pub async fn owner_discord_ids(&self, uuid: &str) -> Result<Vec<i64>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT discord_id FROM members WHERE uuid = $1
             UNION
             SELECT m.discord_id FROM minecraft_accounts ma
             JOIN members m ON m.id = ma.member_id
             WHERE ma.uuid = $1",
        )
        .bind(uuid)
        .fetch_all(self.pool)
        .await
    }

    pub async fn is_owned_by(&self, uuid: &str, discord_id: i64) -> Result<bool, sqlx::Error> {
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM members WHERE discord_id = $2 AND uuid = $1
                UNION ALL
                SELECT 1 FROM minecraft_accounts ma
                JOIN members m ON m.id = ma.member_id
                WHERE ma.uuid = $1 AND m.discord_id = $2
            )",
        )
        .bind(uuid)
        .bind(discord_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.is_some_and(|(exists,)| exists))
    }
}

async fn take_primary(
    conn: &mut PgConnection,
    member_id: i64,
    uuid: &str,
) -> Result<Vec<(i64, i64)>, sqlx::Error> {
    let losers: Vec<(i64, i64)> = sqlx::query_as(
        "UPDATE members m SET uuid = (
           SELECT ma.uuid FROM minecraft_accounts ma
           WHERE ma.member_id = m.id AND ma.uuid <> $1
           ORDER BY ma.added_at, ma.id LIMIT 1)
         WHERE m.uuid = $1 AND m.id <> $2
         RETURNING m.id, m.discord_id",
    )
    .bind(uuid)
    .bind(member_id)
    .fetch_all(&mut *conn)
    .await?;
    let ids: Vec<i64> = losers.iter().map(|&(id, _)| id).collect();
    sqlx::query(
        "DELETE FROM minecraft_accounts ma USING members m
         WHERE ma.member_id = m.id AND ma.uuid = m.uuid AND m.id = ANY($1)",
    )
    .bind(&ids)
    .execute(&mut *conn)
    .await?;
    Ok(losers)
}

async fn take_alts(
    conn: &mut PgConnection,
    member_id: i64,
    uuid: &str,
) -> Result<Vec<(i64, i64)>, sqlx::Error> {
    sqlx::query_as(
        "DELETE FROM minecraft_accounts ma USING members m
         WHERE ma.member_id = m.id AND ma.uuid = $1 AND ma.member_id <> $2
         RETURNING m.id, m.discord_id",
    )
    .bind(uuid)
    .bind(member_id)
    .fetch_all(&mut *conn)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemberRepository;

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

    fn uuid(tag: &str) -> String {
        format!("{tag}{:029x}", Utc::now().timestamp_nanos_opt().unwrap())
    }

    async fn member(pool: &PgPool, base: i64) -> (i64, i64) {
        let discord_id = unique(base);
        let member = MemberRepository::new(pool)
            .create(discord_id)
            .await
            .unwrap();
        (member.id, discord_id)
    }

    async fn primary(pool: &PgPool, member_id: i64) -> Option<String> {
        sqlx::query_scalar("SELECT uuid FROM members WHERE id = $1")
            .bind(member_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn alts(pool: &PgPool, member_id: i64) -> Vec<String> {
        AccountRepository::new(pool)
            .list(member_id)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.uuid)
            .collect()
    }

    #[tokio::test]
    async fn claiming_an_account_takes_it_from_everyone_else() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let accounts = AccountRepository::new(&pool);
        let (shared, kept_alt) = (uuid("aaa"), uuid("bbb"));
        let (holder, holder_discord) = member(&pool, 910_000_000_000_000_000).await;
        let (sharer, sharer_discord) = member(&pool, 920_000_000_000_000_000).await;
        let (claimer, _) = member(&pool, 930_000_000_000_000_000).await;

        accounts
            .claim(holder, &shared, LinkRole::Primary)
            .await
            .unwrap();
        accounts.add(holder, &kept_alt).await.unwrap();
        accounts.add(sharer, &shared).await.unwrap();

        let mut losers = accounts
            .claim(claimer, &shared, LinkRole::Primary)
            .await
            .unwrap();
        losers.sort_unstable();
        let mut expected = vec![holder_discord, sharer_discord];
        expected.sort_unstable();

        assert_eq!(losers, expected);
        assert_eq!(primary(&pool, claimer).await, Some(shared.clone()));
        assert_eq!(primary(&pool, holder).await, Some(kept_alt));
        assert!(alts(&pool, holder).await.is_empty());
        assert!(alts(&pool, sharer).await.is_empty());
        assert!(
            accounts
                .claim(claimer, &shared, LinkRole::Primary)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
