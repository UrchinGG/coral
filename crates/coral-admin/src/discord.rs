use anyhow::{Context as _, Result, anyhow};
use serenity::all::*;

use crate::state::AppState;

pub fn sync_http(state: &AppState) -> Result<Http> {
    build_http(
        state.sync_discord_token.as_deref(),
        "CORAL_SYNC_DISCORD_TOKEN / DISCORD_TOKEN not configured",
    )
}

pub fn bot_http(state: &AppState) -> Result<Http> {
    build_http(
        state.discord_token.as_deref(),
        "DISCORD_TOKEN not configured",
    )
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

fn build_http(token: Option<&str>, missing: &str) -> Result<Http> {
    let token: Token = token
        .ok_or_else(|| anyhow!("{missing}"))?
        .parse()
        .context("invalid Discord bot token")?;
    Ok(Http::new(token))
}
