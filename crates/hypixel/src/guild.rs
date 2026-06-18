use serde_json::Value;

use crate::parsing::GuildInfo;

#[derive(Debug, Clone)]
pub struct Guild {
    pub id: String,
    pub name: String,
    pub tag: Option<String>,
    pub tag_color: Option<String>,
    pub level: u32,
    pub experience: u64,
    pub created: Option<i64>,
    pub members: Vec<GuildMember>,
}

#[derive(Debug, Clone)]
pub struct GuildMember {
    pub uuid: String,
    pub rank: Option<String>,
    pub joined: Option<i64>,
    pub weekly_gexp: u64,
}

impl Guild {
    pub fn from_value(raw: &Value) -> Option<Self> {
        let experience = raw["exp"].as_u64().unwrap_or(0);
        Some(Self {
            id: raw["_id"].as_str()?.to_string(),
            name: raw["name"].as_str()?.to_string(),
            tag: raw["tag"].as_str().map(String::from),
            tag_color: raw["tagColor"].as_str().map(String::from),
            level: level_from_exp(experience),
            experience,
            created: raw["created"].as_i64(),
            members: raw["members"]
                .as_array()
                .map(|members| members.iter().filter_map(GuildMember::from_value).collect())
                .unwrap_or_default(),
        })
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }
}

impl GuildMember {
    fn from_value(raw: &Value) -> Option<Self> {
        Some(Self {
            uuid: normalize_uuid(raw["uuid"].as_str()?),
            rank: raw["rank"].as_str().map(String::from),
            joined: raw["joined"].as_i64(),
            weekly_gexp: weekly_gexp(raw),
        })
    }
}

impl GuildInfo {
    pub fn from_guild(raw: &Value, player_uuid: &str) -> Self {
        let target = normalize_uuid(player_uuid);
        let member = raw["members"].as_array().and_then(|members| {
            members.iter().find(|m| {
                m["uuid"]
                    .as_str()
                    .is_some_and(|u| normalize_uuid(u) == target)
            })
        });
        Self {
            name: raw["name"].as_str().map(String::from),
            tag: raw["tag"].as_str().map(String::from),
            tag_color: raw["tagColor"].as_str().map(String::from),
            rank: member.and_then(|m| m["rank"].as_str().map(String::from)),
            joined: member.and_then(|m| m["joined"].as_i64()),
            weekly_gexp: member.map(weekly_gexp),
        }
    }
}

fn weekly_gexp(member: &Value) -> u64 {
    member["expHistory"]
        .as_object()
        .map(|exp| exp.values().filter_map(|v| v.as_u64()).sum())
        .unwrap_or(0)
}

fn level_from_exp(exp: u64) -> u32 {
    const THRESHOLDS: [u64; 15] = [
        100_000, 150_000, 250_000, 500_000, 750_000, 1_000_000, 1_250_000, 1_500_000, 2_000_000,
        2_500_000, 2_500_000, 2_500_000, 2_500_000, 2_500_000, 3_000_000,
    ];
    const PER_LEVEL_AFTER_15: u64 = 3_000_000;
    let mut level = 0;
    let mut remaining = exp;
    for threshold in THRESHOLDS {
        if remaining < threshold {
            return level;
        }
        remaining -= threshold;
        level += 1;
    }
    level + (remaining / PER_LEVEL_AFTER_15) as u32
}

fn normalize_uuid(uuid: &str) -> String {
    uuid.chars()
        .filter(|c| *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}
