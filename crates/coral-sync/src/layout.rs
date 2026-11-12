//! The context's layout and the one-time rewrite of stored templates and rules into it.
//! Version 1 had Hypixel's player object at the root; version 2 has it under `hypixel`.

use anyhow::Result;
use serenity::all::GuildId;

use database::{GuildConfigRepository, SyncSourceRepository};

use crate::framework::Data;

pub const VERSION: i16 = 2;
pub const HYPIXEL: &str = "hypixel";
/// Namespaces Coral fills itself; every other root a version 1 expression names is Hypixel's.
pub const CORAL_NAMESPACES: &[&str] = &["bedwars", "blacklist", "coral", "discord", "standing"];

pub struct Rewrite {
    pub template: (Option<String>, Option<String>),
    pub conditions: Vec<(i64, String, String)>,
}

impl Rewrite {
    pub fn changes_anything(&self) -> bool {
        self.template.0 != self.template.1
            || self
                .conditions
                .iter()
                .any(|(_, before, after)| before != after)
    }
}

pub fn rewrite_template(template: &str, sources: &[String]) -> String {
    expr::prefix_template_fields(template, HYPIXEL, &kept(sources))
}

pub fn rewrite_condition(condition: &str, sources: &[String]) -> String {
    expr::prefix_fields(condition, HYPIXEL, &kept(sources))
}

/// The rewrite a server's version 1 template and rules would get, read but not stored.
pub async fn plan(data: &Data, guild_id: GuildId, sources: &[String]) -> Result<Rewrite> {
    let repo = GuildConfigRepository::new(data.db.pool());
    let key = guild_id.get() as i64;
    let template = repo
        .get(key)
        .await?
        .and_then(|config| config.nickname_template);
    let rewritten = template.as_deref().map(|t| rewrite_template(t, sources));
    let conditions = repo
        .get_role_rules(key)
        .await?
        .into_iter()
        .map(|rule| {
            let after = rewrite_condition(&rule.condition, sources);
            (rule.id, rule.condition, after)
        })
        .collect();
    Ok(Rewrite {
        template: (template, rewritten),
        conditions,
    })
}

/// Rewrites every server still on an older layout. Runs before the bot connects, so
/// no sync reads a version 1 template against a version 2 context.
pub async fn upgrade(data: &Data) -> Result<()> {
    let repo = GuildConfigRepository::new(data.db.pool());
    let sources: Vec<String> = SyncSourceRepository::new(data.db.pool())
        .list()
        .await?
        .into_iter()
        .map(|source| source.name)
        .collect();
    for guild_id in repo.outdated_contexts(VERSION).await? {
        let rewrite = plan(data, GuildId::new(guild_id as u64), &sources).await?;
        let conditions: Vec<(i64, String)> = rewrite
            .conditions
            .into_iter()
            .map(|(id, _, after)| (id, after))
            .collect();
        repo.rewrite_context(
            guild_id,
            VERSION,
            rewrite.template.1.as_deref(),
            &conditions,
        )
        .await?;
        tracing::info!(
            "Rewrote server {guild_id}'s template and rules for context layout {VERSION}"
        );
    }
    Ok(())
}

fn kept(sources: &[String]) -> Vec<&str> {
    CORAL_NAMESPACES
        .iter()
        .copied()
        .chain(sources.iter().map(String::as_str))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::sources::{self, CoralFacts};

    const STARTER: &str = r#"{bedwars.bracket_open}{achievements.bedwars_level}{if stats.Bedwars.active_star == "x": "✫", else: "★"}{bedwars.bracket_close} {displayname} | {discord.name}"#;

    #[tokio::test]
    async fn the_upgrade_rewrites_old_servers_once_and_new_ones_never() {
        let Some(data) = crate::testing::test_data().await else {
            return;
        };
        let repo = GuildConfigRepository::new(data.db.pool());
        let old = chrono::Utc::now().timestamp_nanos_opt().unwrap();
        repo.upsert(old, 1).await.unwrap();
        repo.set_nickname_template(old, Some("{displayname}"))
            .await
            .unwrap();
        repo.add_role_rule(old, 5, "achievements.bedwars_level >= 100", 0)
            .await
            .unwrap();
        sqlx::query("UPDATE guild_config SET context_version = 1 WHERE guild_id = $1")
            .bind(old)
            .execute(data.db.pool())
            .await
            .unwrap();
        let new = old + 1;
        repo.upsert(new, 1).await.unwrap();
        repo.set_nickname_template(new, Some("{hypixel.displayname}"))
            .await
            .unwrap();

        upgrade(&data).await.unwrap();
        upgrade(&data).await.unwrap();

        for guild in [old, new] {
            let config = repo.get(guild).await.unwrap().unwrap();
            assert_eq!(
                config.nickname_template.as_deref(),
                Some("{hypixel.displayname}")
            );
        }
        let rules = repo.get_role_rules(old).await.unwrap();
        assert_eq!(
            rules[0].condition,
            "hypixel.achievements.bedwars_level >= 100"
        );
    }

    #[test]
    fn coral_namespaces_and_sources_stay_and_hypixel_fields_move() {
        let sources = vec!["rbw".to_string()];
        assert_eq!(
            rewrite_condition(
                "achievements.bedwars_level >= 100 and rbw.top10 and blacklist.sniper",
                &sources
            ),
            "hypixel.achievements.bedwars_level >= 100 and rbw.top10 and blacklist.sniper"
        );
    }

    #[test]
    fn a_rewritten_template_renders_the_same_nickname() {
        let player = json!({"displayname": "Nelly", "achievements": {"bedwars_level": 312}, "stats": {"Bedwars": {"active_star": "x"}}});
        let coral = CoralFacts {
            access: 0,
            trusted: false,
            tags: Vec::new(),
            nickname_paused: false,
        };
        let context = sources::context(&player, Some("nelly"), &coral);
        let mut root = player.clone();
        for namespace in CORAL_NAMESPACES {
            root[*namespace] = context[*namespace].clone();
        }
        let rewritten = rewrite_template(STARTER, &[]);
        assert_eq!(
            expr::render_template(&rewritten, &context).to_truncated(32),
            expr::render_template(STARTER, &root).to_truncated(32)
        );
        assert!(rewritten.contains("{hypixel.displayname}"));
    }
}
