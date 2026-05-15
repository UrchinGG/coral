use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use sha2::{Digest, Sha256};


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct StarfishUser {
    pub id: i64,
    pub discord_id: i64,
    pub license_status: String,
    pub github_user_id: Option<i64>,
    pub github_username: Option<String>,
    pub github_linked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow, Serialize)]
pub struct StarfishHwid {
    pub id: i64,
    pub user_id: i64,
    pub hwid_hash: String,
    pub is_active: bool,
    pub registered_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow)]
pub struct StarfishSession {
    pub id: i64,
    pub user_id: i64,
    pub hwid_id: i64,
    pub session_token: String,
    pub core_data: Vec<u8>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub signature: Vec<u8>,
}


#[derive(Debug, Clone, FromRow)]
pub struct StarfishDeviceCode {
    pub id: i64,
    pub device_code: String,
    pub user_code: String,
    pub client_hwid: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow)]
pub struct StarfishRefreshToken {
    pub id: i64,
    pub user_id: i64,
    pub hwid_id: i64,
    pub token_hash: String,
    pub last_used_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}


#[derive(Debug, Clone, FromRow)]
pub struct StarfishHwidComponent {
    pub id: i64,
    pub hwid_id: i64,
    pub machine_guid_hash: Option<String>,
    pub smbios_uuid_hash: Option<String>,
    pub disk_serial_hash: Option<String>,
    pub cpu_id_hash: Option<String>,
    pub baseboard_serial_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}


#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct HwidComponents {
    pub machine_guid: Option<String>,
    pub smbios_uuid: Option<String>,
    pub disk_serial: Option<String>,
    pub cpu_id: Option<String>,
    pub baseboard_serial: Option<String>,
}

impl HwidComponents {
    pub fn match_count(&self, stored: &StarfishHwidComponent) -> usize {
        let mut count = 0;
        if cmp_component(&self.machine_guid, &stored.machine_guid_hash) { count += 1; }
        if cmp_component(&self.smbios_uuid, &stored.smbios_uuid_hash) { count += 1; }
        if cmp_component(&self.disk_serial, &stored.disk_serial_hash) { count += 1; }
        if cmp_component(&self.cpu_id, &stored.cpu_id_hash) { count += 1; }
        if cmp_component(&self.baseboard_serial, &stored.baseboard_serial_hash) { count += 1; }
        count
    }
}


fn hash_component(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)
}

fn cmp_component(incoming: &Option<String>, stored_hash: &Option<String>) -> bool {
    match (incoming, stored_hash) {
        (Some(value), Some(hash)) => hash_component(value) == *hash,
        _ => false,
    }
}


pub struct StarfishRepository<'a> {
    pool: &'a PgPool,
}


impl<'a> StarfishRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self { Self { pool } }


    pub async fn get_user_by_discord_id(&self, discord_id: i64) -> Result<Option<StarfishUser>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_users WHERE discord_id = $1")
            .bind(discord_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn get_user_by_id(&self, id: i64) -> Result<Option<StarfishUser>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_users WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn upsert_user(&self, discord_id: i64) -> Result<StarfishUser, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO starfish_users (discord_id) VALUES ($1)
             ON CONFLICT (discord_id) DO UPDATE SET updated_at = NOW()
             RETURNING *",
        )
        .bind(discord_id)
        .fetch_one(self.pool)
        .await
    }

    pub async fn set_license_status(&self, discord_id: i64, status: &str) -> Result<bool, sqlx::Error> {
        sqlx::query("UPDATE starfish_users SET license_status = $2, updated_at = NOW() WHERE discord_id = $1")
            .bind(discord_id)
            .bind(status)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn delete_user(&self, discord_id: i64) -> Result<bool, sqlx::Error> {
        sqlx::query("DELETE FROM starfish_users WHERE discord_id = $1")
            .bind(discord_id)
            .execute(self.pool)
            .await
            .map(|r| r.rows_affected() > 0)
    }

    pub async fn list_users(&self) -> Result<Vec<StarfishUser>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_users ORDER BY created_at DESC")
            .fetch_all(self.pool)
            .await
    }

    pub async fn link_github(&self, user_id: i64, github_user_id: i64, github_username: &str) -> Result<StarfishUser, sqlx::Error> {
        sqlx::query_as(
            "UPDATE starfish_users
                SET github_user_id = $2, github_username = $3, github_linked_at = NOW(), updated_at = NOW()
                WHERE id = $1
                RETURNING *",
        )
        .bind(user_id)
        .bind(github_user_id)
        .bind(github_username)
        .fetch_one(self.pool)
        .await
    }


    pub async fn get_hwid(&self, user_id: i64, hwid_hash: &str) -> Result<Option<StarfishHwid>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_hwids WHERE user_id = $1 AND hwid_hash = $2")
            .bind(user_id)
            .bind(hwid_hash)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn get_hwid_by_id(&self, id: i64) -> Result<Option<StarfishHwid>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_hwids WHERE id = $1")
            .bind(id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn get_active_hwid(&self, user_id: i64) -> Result<Option<StarfishHwid>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_hwids WHERE user_id = $1 AND is_active = true")
            .bind(user_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn register_hwid(&self, user_id: i64, hwid_hash: &str) -> Result<StarfishHwid, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("UPDATE starfish_hwids SET is_active = false WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query_as(
            "INSERT INTO starfish_hwids (user_id, hwid_hash, is_active) VALUES ($1, $2, true)
             ON CONFLICT (user_id, hwid_hash) DO UPDATE SET is_active = true
             RETURNING *",
        )
        .bind(user_id)
        .bind(hwid_hash)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(result)
    }

    pub async fn activate_hwid(&self, user_id: i64, hwid_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE starfish_hwids SET is_active = (id = $2) WHERE user_id = $1")
            .bind(user_id)
            .bind(hwid_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn hwid_changes_since(&self, user_id: i64, days: i32) -> Result<i64, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM starfish_hwid_changes WHERE user_id = $1 AND changed_at > NOW() - make_interval(days => $2)",
        )
        .bind(user_id)
        .bind(days)
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    pub async fn record_hwid_change(&self, user_id: i64, old_hwid: Option<&str>, new_hwid: &str) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO starfish_hwid_changes (user_id, old_hwid, new_hwid) VALUES ($1, $2, $3)")
            .bind(user_id)
            .bind(old_hwid)
            .bind(new_hwid)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_user_hwids(&self, user_id: i64) -> Result<Vec<StarfishHwid>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_hwids WHERE user_id = $1 ORDER BY registered_at DESC")
            .bind(user_id)
            .fetch_all(self.pool)
            .await
    }


    pub async fn store_hwid_components(&self, hwid_id: i64, c: &HwidComponents) -> Result<(), sqlx::Error> {
        let machine_guid_hash = c.machine_guid.as_ref().map(|v| hash_component(v));
        let smbios_uuid_hash = c.smbios_uuid.as_ref().map(|v| hash_component(v));
        let disk_serial_hash = c.disk_serial.as_ref().map(|v| hash_component(v));
        let cpu_id_hash = c.cpu_id.as_ref().map(|v| hash_component(v));
        let baseboard_serial_hash = c.baseboard_serial.as_ref().map(|v| hash_component(v));

        sqlx::query(
            "INSERT INTO starfish_hwid_components (hwid_id, machine_guid_hash, smbios_uuid_hash, disk_serial_hash, cpu_id_hash, baseboard_serial_hash)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (hwid_id) DO UPDATE SET
                machine_guid_hash = COALESCE(EXCLUDED.machine_guid_hash, starfish_hwid_components.machine_guid_hash),
                smbios_uuid_hash = COALESCE(EXCLUDED.smbios_uuid_hash, starfish_hwid_components.smbios_uuid_hash),
                disk_serial_hash = COALESCE(EXCLUDED.disk_serial_hash, starfish_hwid_components.disk_serial_hash),
                cpu_id_hash = COALESCE(EXCLUDED.cpu_id_hash, starfish_hwid_components.cpu_id_hash),
                baseboard_serial_hash = COALESCE(EXCLUDED.baseboard_serial_hash, starfish_hwid_components.baseboard_serial_hash)",
        )
        .bind(hwid_id)
        .bind(machine_guid_hash)
        .bind(smbios_uuid_hash)
        .bind(disk_serial_hash)
        .bind(cpu_id_hash)
        .bind(baseboard_serial_hash)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_hwid_components(&self, hwid_id: i64) -> Result<Option<StarfishHwidComponent>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_hwid_components WHERE hwid_id = $1")
            .bind(hwid_id)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn find_fuzzy_hwid(&self, user_id: i64, components: &HwidComponents, threshold: usize) -> Result<Option<StarfishHwid>, sqlx::Error> {
        let hwids = self.get_user_hwids(user_id).await?;
        for hwid in hwids {
            if let Some(stored) = self.get_hwid_components(hwid.id).await? {
                if components.match_count(&stored) >= threshold {
                    return Ok(Some(hwid));
                }
            }
        }
        Ok(None)
    }


    pub async fn get_session_by_token(&self, token: &str) -> Result<Option<StarfishSession>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_sessions WHERE session_token = $1")
            .bind(token)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn create_session(
        &self,
        user_id: i64,
        hwid_id: i64,
        session_token: &str,
        core_data: &[u8],
        expires_at: DateTime<Utc>,
        signature: &[u8],
    ) -> Result<StarfishSession, sqlx::Error> {
        sqlx::query_as(
            "INSERT INTO starfish_sessions (user_id, hwid_id, session_token, core_data, expires_at, signature)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
        )
        .bind(user_id)
        .bind(hwid_id)
        .bind(session_token)
        .bind(core_data)
        .bind(expires_at)
        .bind(signature)
        .fetch_one(self.pool)
        .await
    }

    pub async fn update_heartbeat_sliding(&self, token: &str, sliding_hours: i64, max_lifetime_days: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE starfish_sessions SET
                last_heartbeat_at = NOW(),
                expires_at = LEAST(NOW() + ($2::int * INTERVAL '1 hour'), issued_at + ($3::int * INTERVAL '1 day'))
             WHERE session_token = $1",
        )
        .bind(token)
        .bind(sliding_hours as i32)
        .bind(max_lifetime_days as i32)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_user_sessions(&self, user_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM starfish_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }


    pub async fn create_refresh_token(&self, user_id: i64, hwid_id: i64, token_hash: &str) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO starfish_refresh_tokens (user_id, hwid_id, token_hash) VALUES ($1, $2, $3)")
            .bind(user_id)
            .bind(hwid_id)
            .bind(token_hash)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_refresh_token_by_hash(&self, token_hash: &str) -> Result<Option<StarfishRefreshToken>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_refresh_tokens WHERE token_hash = $1")
            .bind(token_hash)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn rotate_refresh_token(&self, old_hash: &str, new_hash: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE starfish_refresh_tokens SET token_hash = $2, last_used_at = NOW() WHERE token_hash = $1")
            .bind(old_hash)
            .bind(new_hash)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_user_refresh_tokens(&self, user_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM starfish_refresh_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }


    pub async fn create_device_code(
        &self,
        device_code: &str,
        user_code: &str,
        client_hwid: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO starfish_device_codes (device_code, user_code, client_hwid, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(device_code)
        .bind(user_code)
        .bind(client_hwid)
        .bind(expires_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_device_code(&self, device_code: &str) -> Result<Option<StarfishDeviceCode>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM starfish_device_codes WHERE device_code = $1")
            .bind(device_code)
            .fetch_optional(self.pool)
            .await
    }

    pub async fn delete_device_code(&self, device_code: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM starfish_device_codes WHERE device_code = $1")
            .bind(device_code)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn cleanup_expired(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM starfish_device_codes WHERE expires_at < NOW()")
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM starfish_sessions WHERE expires_at < NOW()")
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM starfish_refresh_tokens WHERE last_used_at < NOW() - INTERVAL '90 days'")
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_session_valid(&self, token: &str) -> Result<bool, sqlx::Error> {
        let session = self.get_session_by_token(token).await?;
        Ok(session.is_some_and(|s| s.expires_at > Utc::now()))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url).await.ok()
    }

    fn test_discord_id(seed: i64) -> i64 {
        900_000_000_000_000_000 + seed
    }

    async fn cleanup(repo: &StarfishRepository<'_>, discord_id: i64) {
        repo.delete_user(discord_id).await.ok();
    }

    #[tokio::test]
    async fn user_crud() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(1);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();
        assert_eq!(user.discord_id, did);
        assert_eq!(user.license_status, "inactive");

        let found = repo.get_user_by_discord_id(did).await.unwrap().unwrap();
        assert_eq!(found.id, user.id);

        assert!(repo.set_license_status(did, "active").await.unwrap());
        let updated = repo.get_user_by_discord_id(did).await.unwrap().unwrap();
        assert_eq!(updated.license_status, "active");

        let same = repo.upsert_user(did).await.unwrap();
        assert_eq!(same.id, user.id);

        assert!(repo.delete_user(did).await.unwrap());
        assert!(repo.get_user_by_discord_id(did).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn hwid_registration_and_limits() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(2);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();

        let hwid1 = repo.register_hwid(user.id, "aa".repeat(32).as_str()).await.unwrap();
        assert!(hwid1.is_active);

        let hwid1_again = repo.get_hwid(user.id, "aa".repeat(32).as_str()).await.unwrap().unwrap();
        assert_eq!(hwid1_again.id, hwid1.id);

        repo.record_hwid_change(user.id, Some("aa".repeat(32).as_str()), "bb".repeat(32).as_str()).await.unwrap();
        let hwid2 = repo.register_hwid(user.id, "bb".repeat(32).as_str()).await.unwrap();
        assert!(hwid2.is_active);

        let hwid1_check = repo.get_hwid_by_id(hwid1.id).await.unwrap().unwrap();
        assert!(!hwid1_check.is_active);

        let active = repo.get_active_hwid(user.id).await.unwrap().unwrap();
        assert_eq!(active.id, hwid2.id);

        let changes = repo.hwid_changes_since(user.id, 30).await.unwrap();
        assert_eq!(changes, 1);

        cleanup(&repo, did).await;
    }

    #[tokio::test]
    async fn session_lifecycle() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(3);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();
        let hwid = repo.register_hwid(user.id, "cc".repeat(32).as_str()).await.unwrap();

        let expires = Utc::now() + Duration::hours(2);
        let session = repo.create_session(
            user.id, hwid.id, "test_token_123",
            b"core_data_bytes", expires, b"signature_bytes",
        ).await.unwrap();

        assert_eq!(session.session_token, "test_token_123");
        assert!(repo.is_session_valid("test_token_123").await.unwrap());
        assert!(!repo.is_session_valid("nonexistent").await.unwrap());

        let old_expires = repo.get_session_by_token("test_token_123").await.unwrap().unwrap().expires_at;
        repo.update_heartbeat_sliding("test_token_123", 2, 7).await.unwrap();
        let new_session = repo.get_session_by_token("test_token_123").await.unwrap().unwrap();
        assert!(new_session.expires_at >= old_expires);
        assert!(new_session.last_heartbeat_at > session.last_heartbeat_at);

        repo.delete_user_sessions(user.id).await.unwrap();
        assert!(repo.get_session_by_token("test_token_123").await.unwrap().is_none());

        cleanup(&repo, did).await;
    }

    #[tokio::test]
    async fn session_absolute_cap() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(4);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();
        let hwid = repo.register_hwid(user.id, "dd".repeat(32).as_str()).await.unwrap();

        let issued = Utc::now() - Duration::days(6);
        let expires = issued + Duration::hours(2);
        sqlx::query(
            "INSERT INTO starfish_sessions (user_id, hwid_id, session_token, core_data, issued_at, expires_at, signature)
             VALUES ($1, $2, 'cap_test', $3, $4, $5, $6)",
        )
        .bind(user.id).bind(hwid.id).bind(b"km".as_slice())
        .bind(issued).bind(expires).bind(b"sig".as_slice())
        .execute(&pool).await.unwrap();

        repo.update_heartbeat_sliding("cap_test", 2, 7).await.unwrap();
        let session = repo.get_session_by_token("cap_test").await.unwrap().unwrap();
        let max_allowed = issued + Duration::days(7);
        assert!(session.expires_at <= max_allowed + Duration::seconds(5));
        assert!(session.expires_at < Utc::now() + Duration::hours(2));

        cleanup(&repo, did).await;
    }

    #[tokio::test]
    async fn refresh_token_lifecycle() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(5);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();
        let hwid = repo.register_hwid(user.id, "ee".repeat(32).as_str()).await.unwrap();

        repo.create_refresh_token(user.id, hwid.id, "hash_original").await.unwrap();
        let found = repo.get_refresh_token_by_hash("hash_original").await.unwrap().unwrap();
        assert_eq!(found.user_id, user.id);

        repo.rotate_refresh_token("hash_original", "hash_rotated").await.unwrap();
        assert!(repo.get_refresh_token_by_hash("hash_original").await.unwrap().is_none());
        let rotated = repo.get_refresh_token_by_hash("hash_rotated").await.unwrap().unwrap();
        assert_eq!(rotated.user_id, user.id);
        assert!(rotated.last_used_at >= found.last_used_at);

        repo.delete_user_refresh_tokens(user.id).await.unwrap();
        assert!(repo.get_refresh_token_by_hash("hash_rotated").await.unwrap().is_none());

        cleanup(&repo, did).await;
    }

    #[tokio::test]
    async fn device_code_lifecycle() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);

        let code = format!("test_device_{}", Utc::now().timestamp_millis());
        let expires = Utc::now() + Duration::minutes(10);

        repo.create_device_code(&code, "USR123", "ff".repeat(32).as_str(), expires).await.unwrap();
        let found = repo.get_device_code(&code).await.unwrap().unwrap();
        assert_eq!(found.user_code, "USR123");

        repo.delete_device_code(&code).await.unwrap();
        assert!(repo.get_device_code(&code).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn cascade_delete_cleans_everything() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(7);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();
        let hwid = repo.register_hwid(user.id, "ff".repeat(32).as_str()).await.unwrap();

        let expires = Utc::now() + Duration::hours(2);
        repo.create_session(user.id, hwid.id, "cascade_sess", b"km", expires, b"sig").await.unwrap();
        repo.create_refresh_token(user.id, hwid.id, "cascade_hash").await.unwrap();

        repo.delete_user(did).await.unwrap();

        assert!(repo.get_hwid_by_id(hwid.id).await.unwrap().is_none());
        assert!(repo.get_session_by_token("cascade_sess").await.unwrap().is_none());
        assert!(repo.get_refresh_token_by_hash("cascade_hash").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn revocation_deletes_sessions_and_tokens() {
        let Some(pool) = test_pool().await else { return };
        let repo = StarfishRepository::new(&pool);
        let did = test_discord_id(8);
        cleanup(&repo, did).await;

        let user = repo.upsert_user(did).await.unwrap();
        repo.set_license_status(did, "active").await.unwrap();
        let hwid = repo.register_hwid(user.id, "ab".repeat(32).as_str()).await.unwrap();

        let expires = Utc::now() + Duration::hours(2);
        repo.create_session(user.id, hwid.id, "revoke_sess", b"km", expires, b"sig").await.unwrap();
        repo.create_refresh_token(user.id, hwid.id, "revoke_hash").await.unwrap();

        repo.set_license_status(did, "suspended").await.unwrap();
        repo.delete_user_sessions(user.id).await.unwrap();
        repo.delete_user_refresh_tokens(user.id).await.unwrap();

        let updated = repo.get_user_by_discord_id(did).await.unwrap().unwrap();
        assert_eq!(updated.license_status, "suspended");
        assert!(repo.get_session_by_token("revoke_sess").await.unwrap().is_none());
        assert!(repo.get_refresh_token_by_hash("revoke_hash").await.unwrap().is_none());
        assert!(repo.get_user_by_discord_id(did).await.unwrap().is_some());

        cleanup(&repo, did).await;
    }
}
