use std::collections::HashMap;

use serde::Serialize;
use utoipa::ToSchema;

use crate::discord::DiscordResolver;

#[derive(Serialize, ToSchema)]
pub struct PlayerStatsResponse {
    pub uuid: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<serde_json::Value>)]
    pub hypixel: Option<serde_json::Value>,
    pub tags: Vec<TagResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skin_url: Option<String>,
    pub slim: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
}

#[derive(Serialize, ToSchema)]
pub struct PlayerTagsResponse {
    pub uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    pub tags: Vec<TagResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct TagResponse {
    pub tag_type: String,
    pub reason: String,
    pub added_by: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by_username: Option<String>,
    pub added_on: i64,
    pub hide_username: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CubelifyResponse {
    pub score: CubelifyScore,
    pub tags: Vec<CubelifyTag>,
}

#[derive(Serialize, ToSchema)]
pub struct CubelifyScore {
    pub value: f64,
    pub mode: &'static str,
}

#[derive(Serialize, ToSchema)]
pub struct CubelifyTag {
    pub icon: String,
    pub color: u32,
    pub tooltip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl CubelifyResponse {
    pub fn error(message: &str, icon: &str) -> Self {
        Self {
            score: CubelifyScore {
                value: 0.0,
                mode: "add",
            },
            tags: vec![CubelifyTag {
                icon: icon.to_string(),
                color: 0xFF0000,
                tooltip: message.to_string(),
                text: None,
            }],
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct SuccessResponse {
    pub success: bool,
}

impl TagResponse {
    pub fn from_db(tag: &database::PlayerEvent) -> Self {
        Self {
            tag_type: tag.tag_type.clone().unwrap_or_default(),
            reason: tag.reason.clone().unwrap_or_default(),
            added_by: tag.author.unwrap_or(0),
            added_by_username: None,
            added_on: tag.ts.timestamp_millis(),
            hide_username: tag.hide_username.unwrap_or(false),
            expires_at: tag.expires_at.map(|t| t.timestamp_millis()),
        }
    }
}

pub async fn tag_responses(
    tags: &[database::PlayerEvent],
    discord: &DiscordResolver,
    names: &mut HashMap<i64, Option<String>>,
) -> Vec<TagResponse> {
    let mut out = Vec::with_capacity(tags.len());
    for tag in tags {
        let mut resp = TagResponse::from_db(tag);
        if let Some(author) = tag.author.filter(|_| !tag.hide_username.unwrap_or(false)) {
            resp.added_by_username = match names.get(&author) {
                Some(name) => name.clone(),
                None => {
                    let name = discord.resolve_username(author as u64).await;
                    names.insert(author, name.clone());
                    name
                }
            };
        }
        out.push(resp);
    }
    out
}
