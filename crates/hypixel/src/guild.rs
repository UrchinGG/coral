use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
    pub tag: Option<String>,
    pub tag_color: Option<String>,
    pub created: DateTime<Utc>,
    pub members: Vec<GuildMember>,
    pub experience: u64,
    pub level: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildMember {
    pub uuid: String,
    pub rank: String,
    pub joined: DateTime<Utc>,
}
