use sqlx::PgConnection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum History {
    Keys,
    Links,
}

const SYNC_KEYS: &str = "
WITH active AS (
  SELECT id AS member_id, 'personal' AS kind, api_key, now() AS created_at
  FROM members WHERE id = $1 AND api_key IS NOT NULL
  UNION ALL
  SELECT member_id, 'developer', api_key, created_at FROM developer_keys WHERE member_id = $1
),
retired AS (
  UPDATE member_api_keys k SET retired_at = now()
  WHERE k.member_id = $1 AND k.retired_at IS NULL
    AND NOT EXISTS (SELECT 1 FROM active a WHERE a.api_key = k.api_key)
)
INSERT INTO member_api_keys (member_id, kind, api_key, created_at)
SELECT member_id, kind, api_key, created_at FROM active
ON CONFLICT (api_key) DO NOTHING";

const SYNC_LINKS: &str = "
WITH active AS (
  SELECT id AS member_id, uuid, 'primary' AS relation FROM members WHERE id = $1 AND uuid IS NOT NULL
  UNION ALL
  SELECT member_id, uuid, 'alt' FROM minecraft_accounts WHERE member_id = $1
),
unlinked AS (
  UPDATE member_mc_links l SET unlinked_at = now()
  WHERE l.member_id = $1 AND l.unlinked_at IS NULL
    AND NOT EXISTS (SELECT 1 FROM active a WHERE a.uuid = l.uuid AND a.relation = l.relation)
)
INSERT INTO member_mc_links (member_id, uuid, relation, linked_at)
SELECT member_id, uuid, relation, now() FROM active
ON CONFLICT (member_id, uuid, relation) WHERE unlinked_at IS NULL DO NOTHING";

impl History {
    pub(crate) async fn sync(
        self,
        conn: &mut PgConnection,
        member_id: i64,
    ) -> Result<(), sqlx::Error> {
        let statement = match self {
            Self::Keys => SYNC_KEYS,
            Self::Links => SYNC_LINKS,
        };
        sqlx::query(statement).bind(member_id).execute(conn).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use crate::{AccountRepository, DeveloperKeyRepository, MemberRepository};

    const BASE_DISCORD_ID: i64 = 910_000_000_000_000_000;
    const ID_SPREAD: i64 = 1_000_000_000_000;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        PgPool::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()
    }

    async fn keys(pool: &PgPool, member_id: i64) -> Vec<(String, String, bool)> {
        sqlx::query_as(
            "SELECT kind, api_key, retired_at IS NULL FROM member_api_keys
             WHERE member_id = $1 ORDER BY id",
        )
        .bind(member_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn links(pool: &PgPool, member_id: i64) -> Vec<(String, String, bool)> {
        sqlx::query_as(
            "SELECT relation, uuid, unlinked_at IS NULL FROM member_mc_links
             WHERE member_id = $1 ORDER BY id",
        )
        .bind(member_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn key_and_link_history_follow_repository_writes() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let discord_id =
            BASE_DISCORD_ID + chrono::Utc::now().timestamp_subsec_nanos() as i64 % ID_SPREAD;
        let members = MemberRepository::new(&pool);
        let member = members.create(discord_id).await.unwrap();
        let tag = discord_id.to_string();
        let (first, second) = (format!("first-{tag}"), format!("second-{tag}"));

        members.set_api_key(discord_id, &first).await.unwrap();
        members.set_api_key(discord_id, &second).await.unwrap();
        members.set_uuid(discord_id, "a1").await.unwrap();
        members.set_uuid(discord_id, "a2").await.unwrap();
        members.clear_uuid(discord_id).await.unwrap();
        let accounts = AccountRepository::new(&pool);
        accounts.add(member.id, "alt1").await.unwrap();
        accounts.remove(member.id, "alt1").await.unwrap();
        let developer = DeveloperKeyRepository::new(&pool);
        let (dev_first, dev_second) = (format!("dev1-{tag}"), format!("dev2-{tag}"));
        developer
            .create(member.id, &dev_first, "test", 0, 60)
            .await
            .unwrap();
        developer.set_api_key(member.id, &dev_second).await.unwrap();
        developer.delete(member.id).await.unwrap();

        assert_eq!(
            keys(&pool, member.id).await,
            vec![
                ("personal".into(), first, false),
                ("personal".into(), second, true),
                ("developer".into(), dev_first, false),
                ("developer".into(), dev_second, false),
            ]
        );
        assert_eq!(
            links(&pool, member.id).await,
            vec![
                ("primary".into(), "a1".into(), false),
                ("primary".into(), "a2".into(), false),
                ("alt".into(), "alt1".into(), false),
            ]
        );

        sqlx::query("DELETE FROM members WHERE id = $1")
            .bind(member.id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
