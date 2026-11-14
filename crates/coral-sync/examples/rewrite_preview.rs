//! Prints every server's nickname template and autorole rules before and after the
//! one-time rewrite to the `hypixel.*` layout. Reads only.
//!
//!   DATABASE_URL=… cargo run -p coral-sync --example rewrite_preview

use anyhow::Result;
use serenity::all::GuildId;

use coral_sync::layout;
use database::{Database, GuildConfigRepository};

#[tokio::main]
async fn main() -> Result<()> {
    let db = Database::connect(&std::env::var("DATABASE_URL")?).await?;
    let repo = GuildConfigRepository::new(db.pool());
    for config in repo.get_all().await? {
        let guild_id = GuildId::new(config.guild_id as u64);
        println!("## Server {guild_id}");
        if let Some(template) = &config.nickname_template {
            print_change(
                "template",
                template,
                &layout::rewrite_template(template, &[]),
            );
        }
        for rule in repo.get_role_rules(config.guild_id).await? {
            let after = layout::rewrite_condition(&rule.condition, &[]);
            print_change(
                &format!("rule {} (role {})", rule.id, rule.role_id),
                &rule.condition,
                &after,
            );
        }
        println!();
    }
    Ok(())
}

fn print_change(what: &str, before: &str, after: &str) {
    let valid = if after.contains('{') {
        expr::validate_template(after).is_ok()
    } else {
        expr::validate_condition(after).is_ok()
    };
    println!("{what} (valid after: {valid})\n  before: {before}\n  after:  {after}");
}
