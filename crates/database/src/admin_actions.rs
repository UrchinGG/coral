use chrono::{DateTime, Utc};
use serde::Serializer;
use serde_json::Value;
use sqlx::{FromRow, PgPool};

fn serialize_actor<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct AdminAction {
    pub id: i64,
    #[serde(serialize_with = "serialize_actor")]
    pub actor: i64,
    pub action: String,
    pub target: String,
    pub details: Value,
    pub ts: DateTime<Utc>,
}

pub struct AdminActionRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> AdminActionRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn log(
        &self,
        actor: i64,
        action: &str,
        target: &str,
        details: &Value,
    ) -> Result<AdminAction, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO admin_actions (actor, action, target, details)
             VALUES ($1, $2, $3, $4)
             RETURNING id, actor, action, target, details, ts",
        )
        .bind(actor)
        .bind(action)
        .bind(target)
        .bind(details)
        .fetch_one(self.pool)
        .await
    }

    pub async fn list_recent(&self, limit: i64) -> Result<Vec<AdminAction>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, actor, action, target, details, ts
             FROM admin_actions ORDER BY ts DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await
    }

    pub async fn list_for_target(&self, target: &str) -> Result<Vec<AdminAction>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, actor, action, target, details, ts
             FROM admin_actions WHERE target = $1 ORDER BY ts DESC",
        )
        .bind(target)
        .fetch_all(self.pool)
        .await
    }
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

    async fn cleanup(pool: &PgPool, target: &str) {
        sqlx::query("DELETE FROM admin_actions WHERE target = $1")
            .bind(target)
            .execute(pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn log_and_list_recent() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = AdminActionRepository::new(&pool);
        let target = format!("test_target_{}", Utc::now().timestamp_nanos_opt().unwrap());
        cleanup(&pool, &target).await;

        let action = repo
            .log(
                1,
                "lock_member",
                &target,
                &serde_json::json!({"reason": "test"}),
            )
            .await
            .unwrap();
        assert_eq!(action.actor, 1);
        assert_eq!(action.action, "lock_member");

        let recent = repo.list_recent(10).await.unwrap();
        assert!(recent.iter().any(|a| a.target == target));

        let for_target = repo.list_for_target(&target).await.unwrap();
        assert_eq!(for_target.len(), 1);
        assert_eq!(for_target[0].details["reason"], "test");

        cleanup(&pool, &target).await;
    }

    #[tokio::test]
    async fn list_recent_orders_newest_first() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = AdminActionRepository::new(&pool);
        let target = format!("test_order_{}", Utc::now().timestamp_nanos_opt().unwrap());
        cleanup(&pool, &target).await;

        repo.log(1, "first", &target, &serde_json::json!({}))
            .await
            .unwrap();
        repo.log(1, "second", &target, &serde_json::json!({}))
            .await
            .unwrap();

        let events = repo.list_for_target(&target).await.unwrap();
        assert_eq!(events[0].action, "second");
        assert_eq!(events[1].action, "first");

        cleanup(&pool, &target).await;
    }
}
