use axum::http::{HeaderMap, header};
use coral_redis::web_session::{self, WebUser};
use redis::AsyncCommands;
use reqwest::Url;

use crate::{error::ApiError, state::AppState};

pub const SITE: &str = "https://urchin.gg/";

const DOMAIN: &str = "urchin.gg";
const STATE_COOKIE: &str = "__Secure-urchin_signin";
const STATE_TTL_SECS: u64 = 10 * 60;

pub fn is_urchin(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host == DOMAIN || host.ends_with(&format!(".{DOMAIN}")))
    })
}

pub fn return_address(requested: Option<&str>) -> String {
    requested
        .filter(|url| is_urchin(url))
        .unwrap_or(SITE)
        .to_string()
}

pub fn random_token() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

pub async fn start_sign_in(state: &AppState, nonce: &str, return_to: &str) -> Result<(), ApiError> {
    state
        .redis
        .connection()
        .set_ex::<_, _, ()>(state_key(nonce), return_to, STATE_TTL_SECS)
        .await
        .map_err(redis_error)
}

pub async fn finish_sign_in(state: &AppState, headers: &HeaderMap, nonce: &str) -> Option<String> {
    if cookie(headers, STATE_COOKIE) != Some(nonce) {
        return None;
    }
    state
        .redis
        .connection()
        .get_del::<_, Option<String>>(state_key(nonce))
        .await
        .ok()
        .flatten()
}

pub async fn start_session(state: &AppState, user: &WebUser) -> Result<String, ApiError> {
    web_session::start(&state.redis, user)
        .await
        .map_err(redis_error)
}

pub async fn current_user(state: &AppState, headers: &HeaderMap) -> Option<WebUser> {
    web_session::user(&state.redis, cookie(headers, web_session::COOKIE)?).await
}

pub async fn end_session(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(token) = cookie(headers, web_session::COOKIE) else {
        return Ok(());
    };
    web_session::end(&state.redis, token)
        .await
        .map_err(redis_error)
}

pub fn state_cookie(nonce: &str) -> String {
    format!(
        "{STATE_COOKIE}={nonce}; Path=/auth/discord; HttpOnly; Secure; SameSite=Lax; Max-Age={STATE_TTL_SECS}"
    )
}

pub fn cleared_state_cookie() -> String {
    format!("{STATE_COOKIE}=; Path=/auth/discord; HttpOnly; Secure; SameSite=Lax; Max-Age=0")
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(name)?.strip_prefix('='))
        .filter(|value| !value.is_empty())
}

fn state_key(nonce: &str) -> String {
    format!("web:signin:{nonce}")
}

fn redis_error(error: redis::RedisError) -> ApiError {
    ApiError::Internal(format!("Redis error: {error}"))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn urchin_addresses_are_the_site_and_its_subdomains_over_https() {
        for url in [
            "https://urchin.gg",
            "https://urchin.gg/replays?src=x",
            "https://starfish.urchin.gg/",
            "https://pr-12.urchin.gg/docs",
        ] {
            assert!(is_urchin(url), "{url}");
        }
        for url in [
            "http://urchin.gg",
            "https://evil-urchin.gg",
            "https://urchin.gg.evil.com",
            "https://evil.com/?next=https://urchin.gg",
            "javascript:alert(1)",
            "/relative",
            "",
        ] {
            assert!(!is_urchin(url), "{url}");
        }
    }

    #[test]
    fn return_address_falls_back_to_the_site() {
        assert_eq!(
            return_address(Some("https://starfish.urchin.gg/x")),
            "https://starfish.urchin.gg/x"
        );
        assert_eq!(return_address(Some("https://evil.com")), SITE);
        assert_eq!(return_address(None), SITE);
    }

    #[test]
    fn cookie_finds_the_exact_cookie_and_ignores_empty_values() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Secure-urchin_sessionx=1; __Secure-urchin_signin="),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Secure-urchin_session=abc"),
        );
        assert_eq!(cookie(&headers, web_session::COOKIE), Some("abc"));
        assert_eq!(cookie(&headers, STATE_COOKIE), None);
    }
}
