use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub uuid: Uuid,
    pub username: String,
    pub rank: Option<String>,
    pub network_level: f64,
    pub first_login: Option<DateTime<Utc>>,
    pub last_login: Option<DateTime<Utc>>,
}
