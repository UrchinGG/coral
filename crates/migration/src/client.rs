use anyhow::Result;
use serde_json::Value;

pub struct CoralClient {
    http: reqwest::Client,
    url: String,
    api_key: String,
}

impl CoralClient {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: format!("{base_url}/migrate"),
            api_key: api_key.to_string(),
        }
    }

    pub async fn post(&self, payload: &Value) -> Result<Value> {
        let resp = self.http.post(&self.url)
            .header("X-API-Key", &self.api_key)
            .json(payload)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            anyhow::bail!("API returned {status}: {body}");
        }

        Ok(serde_json::from_str(&body)?)
    }
}
