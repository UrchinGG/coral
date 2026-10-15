use std::collections::BTreeMap;

use database::Database;
use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;
use url::form_urlencoded;

use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

pub const API_KEY_HEADER: &str = "x-api-key";
pub const SESSION_HEADER: &str = "x-starfish-session";

const KEY_QUERY_PARAM: &str = "key";
const SESSION_BODY_FIELD: &str = "session_token";
const PLUGIN_SLUG_SEGMENT: &str = "{slug}";
const PATH_SEPARATOR: char = '/';

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallerDetail {
    pub account: Option<EntityRef>,
    pub actor: Option<EntityRef>,
    pub key: Option<KeyDetail>,
    pub starfish_user: Option<EntityRef>,
    pub plugin: Option<EntityRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyDetail {
    pub key: String,
    pub key_id: Option<i64>,
    pub owner: Option<EntityRef>,
    pub state: KeyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Current,
    Retired,
    Unknown,
}

#[derive(FromRow)]
struct KnownKey {
    key_id: i64,
    member_id: i64,
    discord_id: i64,
    retired: bool,
}

#[derive(FromRow)]
struct StarfishUser {
    discord_id: i64,
    member_id: Option<i64>,
}

pub fn presented_key(
    headers: Option<&BTreeMap<String, String>>,
    query: Option<&str>,
) -> Option<String> {
    if let Some(key) = headers.and_then(|headers| headers.get(API_KEY_HEADER)) {
        return Some(key.clone());
    }
    form_urlencoded::parse(query?.as_bytes())
        .find(|(name, _)| name == KEY_QUERY_PARAM)
        .map(|(_, key)| key.into_owned())
}

pub fn session_token(
    headers: Option<&BTreeMap<String, String>>,
    body: Option<&Value>,
) -> Option<String> {
    if let Some(token) = headers.and_then(|headers| headers.get(SESSION_HEADER)) {
        return Some(token.clone());
    }
    body?.get(SESSION_BODY_FIELD)?.as_str().map(str::to_owned)
}

pub fn plugin_slug(template: Option<&str>, path: &str) -> Option<String> {
    let position = template?
        .split(PATH_SEPARATOR)
        .position(|segment| segment == PLUGIN_SLUG_SEGMENT)?;
    path.split(PATH_SEPARATOR)
        .nth(position)
        .filter(|slug| !slug.is_empty())
        .map(str::to_owned)
}

pub async fn key_detail(db: &Database, key: String) -> Result<KeyDetail, AdminError> {
    let known: Option<KnownKey> = sqlx::query_as(
        "SELECT k.id AS key_id, k.member_id, m.discord_id, k.retired_at IS NOT NULL AS retired
         FROM member_api_keys k JOIN members m ON m.id = k.member_id
         WHERE k.api_key = $1",
    )
    .bind(&key)
    .fetch_optional(db.pool())
    .await
    .context("look up the presented key")?;
    let Some(known) = known else {
        return Ok(KeyDetail {
            key,
            key_id: None,
            owner: None,
            state: KeyState::Unknown,
        });
    };
    Ok(KeyDetail {
        key,
        key_id: Some(known.key_id),
        owner: Some(EntityRef::Account {
            discord_id: known.discord_id,
            member_id: Some(known.member_id),
        }),
        state: match known.retired {
            true => KeyState::Retired,
            false => KeyState::Current,
        },
    })
}

pub async fn starfish_user(db: &Database, token: &str) -> Result<Option<EntityRef>, AdminError> {
    let user: Option<StarfishUser> = sqlx::query_as(
        "SELECT u.discord_id, m.id AS member_id
         FROM starfish_sessions s
         JOIN starfish_users u ON u.id = s.user_id
         LEFT JOIN members m ON m.discord_id = u.discord_id
         WHERE s.session_token = $1",
    )
    .bind(token)
    .fetch_optional(db.pool())
    .await
    .context("look up the Starfish session")?;
    Ok(user.map(|user| EntityRef::Account {
        discord_id: user.discord_id,
        member_id: user.member_id,
    }))
}

pub async fn plugin(db: &Database, slug: String) -> Result<EntityRef, AdminError> {
    let name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM plugins WHERE slug = $1")
            .bind(&slug)
            .fetch_optional(db.pool())
            .await
            .context("look up the plugin")?;
    Ok(EntityRef::Plugin { slug, name })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn the_key_header_wins_over_the_query_parameter() {
        let sent = headers(&[(API_KEY_HEADER, "from-header")]);
        assert_eq!(
            presented_key(Some(&sent), Some("key=from-query")).as_deref(),
            Some("from-header")
        );
        assert_eq!(
            presented_key(None, Some("uuid=x&key=from-query")).as_deref(),
            Some("from-query")
        );
        assert_eq!(
            presented_key(None, Some("key=a%2Db")).as_deref(),
            Some("a-b")
        );
        assert_eq!(presented_key(None, Some("monkey=x")), None);
        assert_eq!(presented_key(None, None), None);
    }

    #[test]
    fn session_tokens_come_from_the_header_or_the_body() {
        let sent = headers(&[(SESSION_HEADER, "header-token")]);
        let body = json!({ "session_token": "body-token" });
        assert_eq!(
            session_token(Some(&sent), Some(&body)).as_deref(),
            Some("header-token")
        );
        assert_eq!(
            session_token(None, Some(&body)).as_deref(),
            Some("body-token")
        );
        assert_eq!(
            session_token(None, Some(&json!({ "session_token": 5 }))),
            None
        );
        assert_eq!(session_token(None, None), None);
    }

    #[test]
    fn plugin_slugs_are_read_from_the_slug_segment() {
        assert_eq!(
            plugin_slug(
                Some("/api/v1/starfish/plugins/{slug}/releases/{version}"),
                "/api/v1/starfish/plugins/bedwars-stats/releases/1.2.0"
            )
            .as_deref(),
            Some("bedwars-stats")
        );
        assert_eq!(
            plugin_slug(Some("/v3/player/tags"), "/v3/player/tags"),
            None
        );
        assert_eq!(plugin_slug(None, "/api/v1/starfish/plugins/x"), None);
    }
}
