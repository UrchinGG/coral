use anyhow::{Result, bail};
use rand::Rng;
use reqwest::Client;
use reqwest::header;

const USER_AGENT: &str = "mc-verify";
const CODE_MIN: u16 = 1000;
const CODE_MAX: u16 = 9999;
const MAX_ATTEMPTS: usize = 100;

#[derive(Clone)]
pub struct CodeStore {
    http: Client,
    api_url: String,
    api_key: String,
}

impl CodeStore {
    pub fn new(http: Client, api_url: String, api_key: String) -> Self {
        Self {
            http,
            api_url,
            api_key,
        }
    }

    pub async fn insert(&self, uuid: &str, username: &str) -> Result<String> {
        let url = format!("{}/v3/verify/codes", self.api_url);
        for _ in 0..MAX_ATTEMPTS {
            let code = generate_code();
            let resp = self
                .http
                .post(&url)
                .header("X-API-Key", &self.api_key)
                .header(header::USER_AGENT, USER_AGENT)
                .json(&serde_json::json!({
                    "code": code,
                    "uuid": uuid,
                    "username": username,
                }))
                .send()
                .await?;
            if resp.status().is_success() {
                return Ok(code);
            }
        }
        bail!("failed to generate unique code after {MAX_ATTEMPTS} attempts")
    }
}

fn generate_code() -> String {
    rand::thread_rng()
        .gen_range(CODE_MIN..=CODE_MAX)
        .to_string()
}
