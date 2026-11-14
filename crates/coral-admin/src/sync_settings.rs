//! Client for coral-sync's settings API. The sync owns its tables; the panel reads
//! and changes them only through here.

use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use serde_json::Value;

use crate::error::AdminError;

const URL_ENV: &str = "SYNC_SETTINGS_URL";
const KEY_ENV: &str = "INTERNAL_API_KEY";
const API_KEY_HEADER: &str = "X-API-Key";
const ACTOR_HEADER: &str = "X-Coral-Actor";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct SyncSettings {
    http: Client,
    base_url: String,
    key: String,
}

impl SyncSettings {
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var(URL_ENV).ok().filter(|url| !url.is_empty())?;
        let key = std::env::var(KEY_ENV).ok().filter(|key| !key.is_empty())?;
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to create HTTP client");
        Some(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            key,
        })
    }

    pub async fn read(&self, path: &str) -> Result<Value, AdminError> {
        self.call(Method::GET, path, None, None).await
    }

    /// A write made for `actor`, the staff member's Discord ID.
    pub async fn write(
        &self,
        method: Method,
        path: &str,
        actor: i64,
        body: Option<Value>,
    ) -> Result<Value, AdminError> {
        self.call(method, path, Some(actor), body).await
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        actor: Option<i64>,
        body: Option<Value>,
    ) -> Result<Value, AdminError> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base_url))
            .header(API_KEY_HEADER, &self.key);
        if let Some(actor) = actor {
            request = request.header(ACTOR_HEADER, actor);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| AdminError::Upstream(format!("the sync service is unreachable: {e}")))?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(body);
        }
        let message = body["error"]
            .as_str()
            .unwrap_or("the sync service refused the request")
            .to_string();
        Err(match status {
            StatusCode::BAD_REQUEST => AdminError::BadRequest(message),
            StatusCode::NOT_FOUND => AdminError::NotFound(message),
            StatusCode::UNPROCESSABLE_ENTITY => AdminError::Rejected(message),
            _ => AdminError::Upstream(format!("the sync service answered {status}: {message}")),
        })
    }
}
