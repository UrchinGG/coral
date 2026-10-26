use anyhow::Result;
use async_trait::async_trait;
use axum::http::{HeaderMap, header};
use coral_redis::{RedisPool, web_session};

pub use web_session::COOKIE as SESSION_COOKIE;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn discord_id(&self, token: &str) -> Option<i64>;
    async fn end(&self, token: &str) -> Result<()>;
}

#[async_trait]
impl SessionStore for RedisPool {
    async fn discord_id(&self, token: &str) -> Option<i64> {
        Some(web_session::user(self, token).await?.discord_id)
    }

    async fn end(&self, token: &str) -> Result<()> {
        Ok(web_session::end(self, token).await?)
    }
}

pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(name)?.strip_prefix('='))
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use axum::http::HeaderValue;

    use super::*;

    #[derive(Default)]
    pub struct MemorySessions(Mutex<HashMap<String, i64>>);

    impl MemorySessions {
        pub fn sign_in(&self, discord_id: i64) -> String {
            let mut sessions = self.0.lock().unwrap();
            let token = format!("token-{}", sessions.len());
            sessions.insert(token.clone(), discord_id);
            token
        }

        pub fn is_live(&self, token: &str) -> bool {
            self.0.lock().unwrap().contains_key(token)
        }
    }

    #[async_trait]
    impl SessionStore for MemorySessions {
        async fn discord_id(&self, token: &str) -> Option<i64> {
            self.0.lock().unwrap().get(token).copied()
        }

        async fn end(&self, token: &str) -> Result<()> {
            self.0.lock().unwrap().remove(token);
            Ok(())
        }
    }

    #[test]
    fn cookie_value_finds_the_exact_cookie_across_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("a=1; __Secure-urchin_sessionx=2; b="),
        );
        headers.append(
            header::COOKIE,
            HeaderValue::from_static("__Secure-urchin_session=3"),
        );
        assert_eq!(cookie_value(&headers, SESSION_COOKIE), Some("3"));
        assert_eq!(cookie_value(&headers, "b"), None);
        assert_eq!(cookie_value(&headers, "c"), None);
    }
}
