use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderName, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use coral_redis::{LIMIT_WINDOW_SECS, PERSONAL_RATE_LIMIT};
use database::{
    AccountRepository, CacheRepository, DeveloperKey, DeveloperKeyRepository, Member,
    MemberRepository, StarfishRepository, permissions,
};
use hypixel::extract_rank_prefix;
use serde::Serialize;

use super::signed_in;
use crate::{
    error::ApiError,
    routes::starfish::{
        auth::{HWID_CHANGE_WINDOW_DAYS, HWID_MAX_CHANGES_PER_WINDOW},
        latest_version,
    },
    state::AppState,
};

const DASHBOARD_ORIGIN: &str = "https://urchin.gg";
const SAME_ORIGIN_SITE: &str = "same-origin";
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const NO_STORE: &str = "no-store";
const LICENSED: &str = "active";

#[derive(Serialize)]
pub struct MemberSummary {
    member_id: i64,
    joined_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct LinkedAccount {
    uuid: String,
    name: Option<String>,
    prefix: Option<String>,
    primary: bool,
}

#[derive(Serialize)]
pub struct ApiKey {
    key: String,
    permissions: Option<Vec<&'static str>>,
    used: i64,
    limit: i64,
    window_secs: i64,
    total: i64,
    created_at: Option<DateTime<Utc>>,
    locked: bool,
}

#[derive(Serialize)]
struct Keys {
    personal: Option<ApiKey>,
    developer: Option<ApiKey>,
}

#[derive(Serialize)]
pub struct StarfishDevice {
    licensed: bool,
    changes_left: i64,
    version: Option<String>,
}

pub async fn member_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MemberSummary>, ApiError> {
    let member = member(&state, &headers)
        .await?
        .ok_or_else(|| ApiError::NotFound("You have no Urchin account".into()))?;
    Ok(Json(MemberSummary {
        member_id: member.id,
        joined_at: member.join_date,
    }))
}

pub async fn accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LinkedAccount>>, ApiError> {
    let Some(member) = member(&state, &headers).await? else {
        return Ok(Json(Vec::new()));
    };
    let alts = AccountRepository::new(state.db.pool())
        .list(member.id)
        .await?;
    let linked: Vec<(String, bool)> = member
        .uuid
        .iter()
        .map(|uuid| (uuid.clone(), true))
        .chain(alts.into_iter().map(|alt| (alt.uuid, false)))
        .collect();
    let cache = CacheRepository::new(state.db.pool());
    let uuids: Vec<String> = linked.iter().map(|(uuid, _)| uuid.clone()).collect();
    let mut names = cache.usernames(&uuids).await?;
    let mut accounts = Vec::with_capacity(linked.len());
    for (uuid, primary) in linked {
        let prefix = cache
            .get_latest_snapshot(&uuid)
            .await?
            .and_then(|player| extract_rank_prefix(&player));
        accounts.push(LinkedAccount {
            name: names.remove(&uuid),
            uuid,
            prefix,
            primary,
        });
    }
    Ok(Json(accounts))
}

pub async fn keys(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let Some(member) = member(&state, &headers).await? else {
        return Ok(no_store(Keys {
            personal: None,
            developer: None,
        }));
    };
    let developer = DeveloperKeyRepository::new(state.db.pool())
        .get_by_member_id(member.id)
        .await?;
    let keys = Keys {
        personal: personal_key(&state, &member).await?,
        developer: match developer {
            Some(key) => Some(developer_key(&state, key).await?),
            None => None,
        },
    };
    Ok(no_store(keys))
}

pub async fn regenerate_personal_key(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !from_dashboard(&headers) {
        return Err(ApiError::Forbidden(
            "Regenerate your key from the dashboard".into(),
        ));
    }
    let user = signed_in(&state, &headers).await?;
    let repo = MemberRepository::new(state.db.pool());
    let replaced = repo
        .set_api_key(user.discord_id, &uuid::Uuid::new_v4().to_string())
        .await?;
    let member = repo
        .get_by_discord_id(user.discord_id)
        .await?
        .filter(|_| replaced)
        .ok_or_else(|| ApiError::NotFound("You have no Urchin account".into()))?;
    Ok(no_store(personal_key(&state, &member).await?))
}

pub async fn starfish(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<StarfishDevice>, ApiError> {
    let user = signed_in(&state, &headers).await?;
    let repo = StarfishRepository::new(state.db.pool());
    let starfish = repo
        .get_user_by_discord_id(user.discord_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("You have no Starfish account".into()))?;
    let changes = repo
        .hwid_changes_since(starfish.id, HWID_CHANGE_WINDOW_DAYS)
        .await?;
    Ok(Json(StarfishDevice {
        licensed: starfish.license_status == LICENSED,
        changes_left: changes_left(changes),
        version: latest_version(&state).await,
    }))
}

async fn member(state: &AppState, headers: &HeaderMap) -> Result<Option<Member>, ApiError> {
    let user = signed_in(state, headers).await?;
    Ok(MemberRepository::new(state.db.pool())
        .get_by_discord_id(user.discord_id)
        .await?)
}

async fn personal_key(state: &AppState, member: &Member) -> Result<Option<ApiKey>, ApiError> {
    let Some(key) = member.api_key.clone() else {
        return Ok(None);
    };
    let created_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT created_at FROM member_api_keys WHERE member_id = $1 AND api_key = $2",
    )
    .bind(member.id)
    .bind(&key)
    .fetch_optional(state.db.pool())
    .await?
    .flatten();
    Ok(Some(ApiKey {
        used: used(state, &key).await?,
        key,
        permissions: None,
        limit: PERSONAL_RATE_LIMIT,
        window_secs: LIMIT_WINDOW_SECS,
        total: member.request_count,
        created_at,
        locked: member.key_locked,
    }))
}

async fn developer_key(state: &AppState, key: DeveloperKey) -> Result<ApiKey, ApiError> {
    Ok(ApiKey {
        used: used(state, &key.api_key).await?,
        permissions: Some(permission_labels(key.permissions)),
        key: key.api_key,
        limit: i64::from(key.rate_limit),
        window_secs: LIMIT_WINDOW_SECS,
        total: key.request_count,
        created_at: Some(key.created_at),
        locked: key.locked,
    })
}

async fn used(state: &AppState, key: &str) -> Result<i64, ApiError> {
    let usage = state
        .rate_limiter
        .usage_many(&[key.to_string()])
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("rate limit usage unavailable: {e}")))?;
    Ok(usage.first().copied().unwrap_or_default())
}

fn permission_labels(granted: i64) -> Vec<&'static str> {
    permissions::ALL
        .iter()
        .filter(|&&permission| granted & permission != 0)
        .map(|&permission| permissions::label(permission))
        .collect()
}

fn changes_left(changes: i64) -> i64 {
    (HWID_MAX_CHANGES_PER_WINDOW - changes).max(0)
}

fn from_dashboard(headers: &HeaderMap) -> bool {
    let same_site = headers
        .get(SEC_FETCH_SITE)
        .is_none_or(|site| site == SAME_ORIGIN_SITE);
    let origin = headers
        .get(header::ORIGIN)
        .is_some_and(|origin| origin == DASHBOARD_ORIGIN);
    same_site && origin
}

fn no_store(body: impl Serialize) -> Response {
    ([(header::CACHE_CONTROL, NO_STORE)], Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(pairs: &[(HeaderName, &'static str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| (name.clone(), HeaderValue::from_static(value)))
            .collect()
    }

    #[test]
    fn only_same_origin_dashboard_writes_pass() {
        assert!(from_dashboard(&headers(&[
            (header::ORIGIN, "https://urchin.gg"),
            (SEC_FETCH_SITE, "same-origin"),
        ])));
        assert!(from_dashboard(&headers(&[(
            header::ORIGIN,
            "https://urchin.gg"
        )])));
        assert!(!from_dashboard(&headers(&[
            (header::ORIGIN, "https://urchin.gg"),
            (SEC_FETCH_SITE, "same-site"),
        ])));
        assert!(!from_dashboard(&headers(&[(
            header::ORIGIN,
            "https://pr-3.urchin.gg"
        )])));
        assert!(!from_dashboard(&headers(&[(
            SEC_FETCH_SITE,
            "same-origin"
        )])));
    }

    #[test]
    fn device_changes_left_never_go_negative() {
        assert_eq!(changes_left(0), HWID_MAX_CHANGES_PER_WINDOW);
        assert_eq!(changes_left(HWID_MAX_CHANGES_PER_WINDOW + 3), 0);
    }

    #[test]
    fn developer_permissions_are_listed_by_label() {
        assert_eq!(
            permission_labels(permissions::PLAYER_DATA | permissions::TAGS),
            vec!["Player Data", "Tags"]
        );
        assert!(permission_labels(0).is_empty());
    }
}

#[cfg(test)]
mod api_tests {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use clients::MojangClient;
    use coral_redis::{RedisPool, web_session};
    use database::Database;
    use serde_json::Value;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    const DISCORD_BASE: i64 = 731_000_000_000_000_000;

    async fn state() -> Option<AppState> {
        dotenvy::dotenv().ok();
        let db = Database::connect(&std::env::var("DATABASE_URL").ok()?)
            .await
            .ok()?;
        let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
            .await
            .ok()?;
        Some(AppState::new(
            db,
            None,
            MojangClient::new(redis.connection()),
            None,
            None,
            redis,
            None,
            None,
            None,
        ))
    }

    async fn signed_in_cookie(state: &AppState, discord_id: i64) -> String {
        let user = web_session::WebUser {
            discord_id,
            name: "Kat".into(),
            username: "kat".into(),
            avatar: String::new(),
        };
        let token = web_session::start(&state.redis, &user).await.unwrap();
        format!("{}={token}", web_session::COOKIE)
    }

    async fn call(state: &AppState, method: Method, uri: &str, cookie: &str) -> (u16, Value) {
        let (request_log, _log) = ::reqlog::channel();
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, cookie)
            .header(header::ORIGIN, DASHBOARD_ORIGIN)
            .body(Body::empty())
            .unwrap();
        let response = crate::build_router(state.clone(), request_log)
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn a_member_reads_their_accounts_and_keys_and_regenerates_the_personal_one() {
        let Some(state) = state().await else {
            return;
        };
        let discord_id = DISCORD_BASE + (Uuid::new_v4().as_u128() % 1_000_000_000) as i64;
        let repo = MemberRepository::new(state.db.pool());
        let member = repo.create(discord_id).await.unwrap();
        let primary = Uuid::new_v4().simple().to_string();
        sqlx::query("UPDATE members SET uuid = $2 WHERE id = $1")
            .bind(member.id)
            .bind(&primary)
            .execute(state.db.pool())
            .await
            .unwrap();
        let cookie = signed_in_cookie(&state, discord_id).await;

        let (status, summary) = call(&state, Method::GET, "/me/member", &cookie).await;
        assert_eq!(status, 200);
        assert_eq!(summary["member_id"], member.id);

        let (status, accounts) = call(&state, Method::GET, "/me/accounts", &cookie).await;
        assert_eq!(status, 200);
        assert_eq!(accounts[0]["uuid"], primary.as_str());
        assert_eq!(accounts[0]["primary"], true);

        let (status, keys) = call(&state, Method::GET, "/me/keys", &cookie).await;
        assert_eq!(status, 200);
        assert!(keys["personal"].is_null());
        assert!(keys["developer"].is_null());

        let (status, created) = call(
            &state,
            Method::POST,
            "/me/keys/personal/regenerate",
            &cookie,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(created["limit"], PERSONAL_RATE_LIMIT);

        let (_, keys) = call(&state, Method::GET, "/me/keys", &cookie).await;
        assert_eq!(keys["personal"]["key"], created["key"]);
    }

    #[tokio::test]
    async fn signed_out_callers_get_401() {
        let Some(state) = state().await else {
            return;
        };
        for uri in ["/me/member", "/me/accounts", "/me/keys", "/me/starfish"] {
            let (status, _) = call(&state, Method::GET, uri, "").await;
            assert_eq!(status, 401, "{uri}");
        }
    }
}
