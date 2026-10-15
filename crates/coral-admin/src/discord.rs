use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use serenity::all::*;

const BOT_TOKEN_ENV: &str = "DISCORD_TOKEN";
const SYNC_TOKEN_ENV: &str = "CORAL_SYNC_DISCORD_TOKEN";
const HOME_GUILD_ENV: &str = "HOME_GUILD_ID";
const REVIEW_FORUM_ENV: &str = "REVIEW_FORUM_ID";
const BOT_MISSING: &str = "DISCORD_TOKEN not configured";
const SYNC_MISSING: &str = "CORAL_SYNC_DISCORD_TOKEN / DISCORD_TOKEN not configured";

#[derive(Clone)]
pub struct DiscordContext {
    bot: Option<Arc<Http>>,
    sync: Option<Arc<Http>>,
    pub home_guild_id: Option<i64>,
    pub review_forum_id: Option<i64>,
}

impl DiscordContext {
    pub fn from_env() -> Result<Self> {
        let bot_token = env_non_empty(BOT_TOKEN_ENV);
        let sync_token = env_non_empty(SYNC_TOKEN_ENV);
        let bot = bot_token.as_deref().map(build_http).transpose()?;
        let sync = match sync_token {
            Some(token) if Some(&token) != bot_token.as_ref() => Some(build_http(&token)?),
            _ => bot.clone(),
        };
        Ok(Self {
            bot,
            sync,
            home_guild_id: env_id(HOME_GUILD_ENV),
            review_forum_id: env_id(REVIEW_FORUM_ENV),
        })
    }

    pub fn bot(&self) -> Result<&Http> {
        self.bot.as_deref().ok_or_else(|| anyhow!(BOT_MISSING))
    }

    pub fn sync(&self) -> Result<&Http> {
        self.sync.as_deref().ok_or_else(|| anyhow!(SYNC_MISSING))
    }

    pub fn shared_bot(&self) -> Option<Arc<Http>> {
        self.bot.clone()
    }
}

pub async fn guild_roles(http: &Http, guild_id: GuildId) -> Result<Vec<Role>> {
    Ok(http.get_guild_roles(guild_id).await?.into_iter().collect())
}

pub async fn guild_channels(http: &Http, guild_id: GuildId) -> Result<Vec<GuildChannel>> {
    Ok(http.get_channels(guild_id).await?.into_iter().collect())
}

pub async fn list_guild_members(http: &Http, guild_id: GuildId) -> Result<Vec<Member>> {
    let mut all = Vec::new();
    let mut after = None;
    loop {
        let page = http.get_guild_members(guild_id, None, after).await?;
        let Some(last) = page.last() else { break };
        after = Some(last.user.id);
        let done = page.len() < usize::from(constants::MEMBER_FETCH_LIMIT.get());
        all.extend(page);
        if done {
            break;
        }
    }
    Ok(all)
}

pub fn role_color_hex(colour: Colour) -> Option<String> {
    (colour.0 != 0).then(|| format!("#{:06x}", colour.0))
}

pub fn text(s: impl Into<String>) -> CreateContainerComponent<'static> {
    CreateContainerComponent::TextDisplay(CreateTextDisplay::new(s.into()))
}

pub fn separator() -> CreateContainerComponent<'static> {
    CreateContainerComponent::Separator(CreateSeparator::new(true))
}

fn build_http(token: &str) -> Result<Arc<Http>> {
    let token: Token = token.parse().context("invalid Discord bot token")?;
    Ok(Arc::new(Http::new(token)))
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_id(name: &str) -> Option<i64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}
