use image::DynamicImage;
use serde_json::Value;


pub struct PlayerData {
    pub uuid: String,
    pub username: String,
    pub skin: Option<DynamicImage>,
    pub hypixel: Value,
    pub guild: Value,
}


impl PlayerData {
    pub fn empty() -> Self {
        Self { uuid: String::new(), username: String::new(), skin: None, hypixel: Value::Null, guild: Value::Null }
    }

    pub fn guild_info(&self) -> Option<hypixel::GuildInfo> {
        let g = self.guild.get("guild")?;
        let name = g.get("name")?.as_str()?.to_string();
        let tag = g.get("tag").and_then(|t| t.as_str()).map(String::from);
        let tag_color = g.get("tagColor").and_then(|c| c.as_str()).map(String::from);

        let members = g.get("members")?.as_array()?;
        let member = members.iter().find(|m| {
            m.get("uuid").and_then(|u| u.as_str()).map(|u| u.replace('-', "").to_lowercase())
                == Some(self.uuid.to_lowercase())
        })?;

        let rank = member.get("rank").and_then(|r| r.as_str()).map(String::from);
        let joined = member.get("joined").and_then(|j| j.as_i64());
        let weekly_gexp: Option<u64> = member
            .get("expHistory")
            .and_then(|h| h.as_object().map(|obj| obj.values().filter_map(|v| v.as_u64()).sum()));

        Some(hypixel::GuildInfo { name: Some(name), tag, tag_color, rank, joined, weekly_gexp })
    }
}
