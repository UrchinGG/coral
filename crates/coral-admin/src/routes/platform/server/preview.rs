use database::{
    BlacklistRepository, CacheRepository, DiscordUsernameCacheRepository, MemberRepository,
};
use serde_json::{Value, json};

use crate::error::{AdminError, Context};
use crate::state::AppState;

const BEDWARS_LEVEL: &str = "/achievements/bedwars_level";
const PRESTIGE_BRACKET: &str = "/stats/Bedwars/active_prestige_bracket";

pub async fn preview_context(
    state: &AppState,
    discord_id: i64,
) -> Result<Option<Value>, AdminError> {
    let pool = state.db.pool();
    let Some(member) = MemberRepository::new(pool)
        .get_by_discord_id(discord_id)
        .await
        .context("read the owner's member row")?
    else {
        return Ok(None);
    };
    let Some(uuid) = member.uuid.clone() else {
        return Ok(None);
    };
    let cache = CacheRepository::new(pool);
    let tags = BlacklistRepository::new(pool);
    let names = DiscordUsernameCacheRepository::new(pool);
    let (snapshot, active_tags, cached_name) = tokio::try_join!(
        async {
            cache
                .get_latest_snapshot(&uuid)
                .await
                .context("read the owner's latest snapshot")
        },
        async {
            tags.get_active_tags(&uuid)
                .await
                .context("read the owner's active tags")
        },
        async {
            names
                .get(discord_id)
                .await
                .context("read the owner's cached Discord name")
        },
    )?;
    let Some(mut context) = snapshot else {
        return Ok(None);
    };
    let tag_names: Vec<String> = active_tags
        .into_iter()
        .filter_map(|event| event.tag_type)
        .collect();
    if context.pointer(BEDWARS_LEVEL).is_none() {
        context["achievements"]["bedwars_level"] = json!(0);
    }
    let bracket = context.pointer(PRESTIGE_BRACKET).and_then(Value::as_str);
    let (bracket_open, bracket_close) = hypixel::nickname_bracket_glyphs(bracket);
    context["bedwars"] = json!({ "bracket_open": bracket_open, "bracket_close": bracket_close });
    context["discord"] = json!({ "name": cached_name.map(|cached| cached.username) });
    context["coral"] = json!({ "access": member.access_level });
    context["standing"] = json!({ "trusted": member.tag_granted });
    context["blacklist"] = blacklist_context(&tag_names);
    Ok(Some(context))
}

fn blacklist_context(tag_names: &[String]) -> Value {
    let highest = tag_names
        .iter()
        .filter_map(|name| blacklist::lookup(name).map(|tag| (tag.priority, name.as_str())))
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, name)| name);
    let mut context = json!({ "tag": highest });
    for tag in blacklist::all() {
        context[tag.name] = Value::Bool(tag_names.iter().any(|name| name == tag.name));
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blacklist_context_names_the_highest_priority_tag_and_flags_each_tag() {
        let context = blacklist_context(&["caution".into(), "sniper".into()]);
        assert_eq!(context["tag"], "sniper");
        assert_eq!(context["sniper"], true);
        assert_eq!(context["caution"], true);
        assert_eq!(context["closet_cheater"], false);
    }
}
