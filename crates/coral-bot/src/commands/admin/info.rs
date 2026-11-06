use std::time::{Duration, Instant};

use anyhow::Result;
use serenity::all::*;

use database::{
    BlacklistRepository, CacheRepository, GuildCacheRepository, GuildCurrentRepository,
    MemberRepository,
};

use crate::{
    framework::Data,
    utils::{format_number, separator, text},
};

const CACHE_TTL: Duration = Duration::from_secs(300);

type Page = Vec<CreateComponent<'static>>;

pub type InfoCache = Option<(Page, Instant)>;

async fn statistics_page(ctx: &Context, data: &Data) -> Page {
    if let Some((page, at)) = data.info_cache.lock().unwrap().as_ref()
        && at.elapsed() < CACHE_TTL
    {
        return page.clone();
    }
    let page = build_statistics(ctx, data).await;
    *data.info_cache.lock().unwrap() = Some((page.clone(), Instant::now()));
    page
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;
    command
        .edit_response(&ctx.http, page_response(statistics_page(ctx, data).await))
        .await?;
    Ok(())
}

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("info").description("Urchin statistics")
}

fn page_response(components: Vec<CreateComponent<'static>>) -> EditInteractionResponse<'static> {
    EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .allowed_mentions(CreateAllowedMentions::new())
        .components(components)
}

async fn build_statistics(ctx: &Context, data: &Data) -> Vec<CreateComponent<'static>> {
    let pool = data.db.pool();
    let members = MemberRepository::new(pool);
    let blacklist = BlacklistRepository::new(pool);
    let cache = CacheRepository::new(pool);
    let guild_cache = GuildCacheRepository::new(pool);
    let guilds = GuildCurrentRepository::new(pool);

    let (
        registered,
        trusted,
        requests,
        tags,
        breakdown,
        snapshots,
        tracked,
        storage,
        guild_snapshots,
        guilds_tracked,
        guild_storage,
    ) = tokio::join!(
        members.count(),
        members.count_trusted(),
        members.total_requests(),
        blacklist.count_active_tags(),
        blacklist.count_active_tags_by_type(),
        cache.count_snapshots(),
        cache.count_unique_players(),
        cache.storage_bytes(),
        guild_cache.count_snapshots(),
        guilds.count_guilds(),
        guild_cache.storage_bytes(),
    );

    let tag_lines: Vec<String> = breakdown
        .unwrap_or_default()
        .iter()
        .map(|(tag_type, count)| {
            let emote = blacklist::lookup(tag_type).map(|d| d.emote).unwrap_or("");
            format!(
                "{emote} **{}** {}",
                format_number(*count as u64),
                tag_type.replace('_', " ")
            )
        })
        .collect();
    let tag_display = if tag_lines.is_empty() {
        "No tags yet".into()
    } else {
        tag_lines.join("\n")
    };

    let (servers, users, user_installs, icon) = bot_stats(ctx).await;

    let header = format!(
        "## Info\n**Online**, started <t:{}:R>\n**{}** server installs ({} users)\n**{}** user installs",
        data.started_at,
        format_number(servers),
        format_number(users),
        format_number(user_installs),
    );
    let header = match icon {
        Some(url) => CreateContainerComponent::Section(CreateSection::new(
            vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                header,
            ))],
            CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
                url,
            ))),
        )),
        None => text(header),
    };

    let parts = vec![
        header,
        separator(),
        text(format!(
            "### Members\n**{}** registered\n**{}** trusted\n**{}** lifetime requests",
            format_number(registered.unwrap_or(0) as u64),
            format_number(trusted.unwrap_or(0) as u64),
            format_number(requests.unwrap_or(0) as u64),
        )),
        separator(),
        text(format!(
            "### Blacklist\n**{}** active tags\n{tag_display}",
            format_number(tags.unwrap_or(0) as u64),
        )),
        separator(),
        text(format!(
            "### Guild Cache | `{}`\n**{}** guilds tracked\n**{}** snapshots",
            bytes(guild_storage.unwrap_or(0)),
            format_number(guilds_tracked.unwrap_or(0) as u64),
            format_number(guild_snapshots.unwrap_or(0) as u64),
        )),
        separator(),
        text(format!(
            "### Player Cache | `{}`\n**{}** players tracked\n**{}** snapshots",
            bytes(storage.unwrap_or(0)),
            format_number(tracked.unwrap_or(0) as u64),
            format_number(snapshots.unwrap_or(0) as u64),
        )),
    ];

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn bot_stats(ctx: &Context) -> (u64, u64, u64, Option<String>) {
    let (app, guilds) = tokio::join!(
        ctx.http.get_current_application_info(),
        ctx.http.get_guilds(None, None),
    );
    let guilds = guilds.unwrap_or_default();
    let (servers, user_installs, icon) = match &app {
        Ok(a) => (
            a.approximate_guild_count
                .map(u64::from)
                .unwrap_or(guilds.len() as u64),
            a.approximate_user_install_count.map(u64::from).unwrap_or(0),
            a.icon
                .as_ref()
                .map(|hash| format!("https://cdn.discordapp.com/app-icons/{}/{hash}.png", a.id)),
        ),
        Err(_) => (guilds.len() as u64, 0, None),
    };
    let counts =
        futures_util::future::join_all(guilds.iter().map(|g| ctx.http.get_guild_with_counts(g.id)))
            .await;
    let users = counts
        .into_iter()
        .filter_map(|r| r.ok().and_then(|g| g.approximate_member_count))
        .map(|n| n.get())
        .sum();
    (servers, users, user_installs, icon)
}

fn bytes(value: i64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let value = value as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.0} MB", value / MB)
    } else {
        format!("{:.0} KB", value / 1024.0)
    }
}
