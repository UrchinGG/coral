use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serenity::all::*;

use database::{BlacklistRepository, CacheRepository, MemberRepository, Period};

use crate::{
    framework::Data,
    utils::{format_number, separator, text},
};

const LIMIT: i64 = 10;
const CACHE_TTL: Duration = Duration::from_secs(300);

type Page = Vec<CreateComponent<'static>>;

#[derive(Default)]
pub struct InfoCache {
    statistics: Option<(Page, Instant)>,
    leaderboards: Option<(Page, Instant)>,
}

fn fresh(slot: &Option<(Page, Instant)>) -> Option<Page> {
    slot.as_ref()
        .filter(|(_, at)| at.elapsed() < CACHE_TTL)
        .map(|(page, _)| page.clone())
}

async fn statistics_page(ctx: &Context, data: &Data) -> Page {
    if let Some(page) = fresh(&data.info_cache.lock().unwrap().statistics) {
        return page;
    }
    let page = build_statistics(ctx, data).await;
    data.info_cache.lock().unwrap().statistics = Some((page.clone(), Instant::now()));
    page
}

async fn leaderboards_page(ctx: &Context, data: &Data) -> Page {
    if let Some(page) = fresh(&data.info_cache.lock().unwrap().leaderboards) {
        return page;
    }
    let page = build_leaderboards(ctx, data, None).await;
    data.info_cache.lock().unwrap().leaderboards = Some((page.clone(), Instant::now()));
    page
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;
    command
        .edit_response(&ctx.http, page_response(statistics_page(ctx, data).await))
        .await?;

    let ctx = ctx.clone();
    let data = data.clone();
    tokio::spawn(async move {
        leaderboards_page(&ctx, &data).await;
    });
    Ok(())
}

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("info").description("Coral statistics and leaderboards")
}

pub async fn handle_page(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let components = match component.data.custom_id.strip_prefix("info_page:") {
        Some("leaderboards") => leaderboards_page(ctx, data).await,
        _ => statistics_page(ctx, data).await,
    };
    component
        .edit_response(&ctx.http, page_response(components))
        .await?;
    Ok(())
}

pub async fn handle_taggers(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let period = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            match values.first().map(String::as_str) {
                Some("monthly") => Some(Period::Monthly),
                Some("weekly") => Some(Period::Weekly),
                _ => None,
            }
        }
        _ => None,
    };
    let components = match period {
        None => leaderboards_page(ctx, data).await,
        Some(_) => build_leaderboards(ctx, data, period).await,
    };
    component
        .edit_response(&ctx.http, page_response(components))
        .await?;
    Ok(())
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

    let (registered, requests, tags, breakdown, snapshots, tracked, storage) = tokio::join!(
        members.count(),
        members.total_requests(),
        blacklist.count_active_tags(),
        blacklist.count_active_tags_by_type(),
        cache.count_snapshots(),
        cache.count_unique_players(),
        cache.storage_bytes(),
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
            "### Members\n**{}** registered\n**{}** lifetime requests",
            format_number(registered.unwrap_or(0) as u64),
            format_number(requests.unwrap_or(0) as u64),
        )),
        separator(),
        text(format!(
            "### Blacklist\n**{}** active tags\n{tag_display}",
            format_number(tags.unwrap_or(0) as u64),
        )),
        separator(),
        text(format!(
            "### Cache | `{}`\n**{}** players tracked\n**{}** snapshots",
            bytes(storage.unwrap_or(0)),
            format_number(tracked.unwrap_or(0) as u64),
            format_number(snapshots.unwrap_or(0) as u64),
        )),
        nav("statistics"),
    ];

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn build_leaderboards(
    ctx: &Context,
    data: &Data,
    period: Option<Period>,
) -> Vec<CreateComponent<'static>> {
    let pool = data.db.pool();
    let requesters = MemberRepository::new(pool)
        .top_requesters(LIMIT)
        .await
        .unwrap_or_default();
    let since = period.map(|p| p.last_reset(Utc::now()));
    let taggers = BlacklistRepository::new(pool)
        .top_taggers(since, LIMIT)
        .await
        .unwrap_or_default();

    let parts = vec![
        text("## Leaderboards"),
        separator(),
        text(format!(
            "### Top Requesters (All-time)\n{}",
            leaderboard(ctx, &requesters).await
        )),
        separator(),
        text("### Top Taggers"),
        period_dropdown(period),
        text(leaderboard(ctx, &taggers).await),
        nav("leaderboards"),
    ];

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn leaderboard(ctx: &Context, rows: &[(i64, i64)]) -> String {
    if rows.is_empty() {
        return "-# No data yet".into();
    }
    let names = futures_util::future::join_all(
        rows.iter()
            .map(|(id, _)| ctx.http.get_user(UserId::new(*id as u64))),
    )
    .await;
    rows.iter()
        .zip(names)
        .enumerate()
        .map(|(i, ((id, count), user))| {
            let name = user
                .map(|u| u.name.to_string())
                .unwrap_or_else(|_| "unknown".into());
            format!(
                "{}. **{}** — <@{id}> `{name}`",
                i + 1,
                format_number(*count as u64)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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

fn nav(active: &str) -> CreateContainerComponent<'static> {
    let button = |key: &str, label: &'static str| {
        let mut b = CreateButton::new(format!("info_page:{key}")).label(label);
        if key == active {
            b = b.style(ButtonStyle::Primary).disabled(true);
        } else {
            b = b.style(ButtonStyle::Secondary);
        }
        b
    };
    CreateContainerComponent::ActionRow(CreateActionRow::Buttons(
        vec![
            button("statistics", "Statistics"),
            button("leaderboards", "Leaderboards"),
        ]
        .into(),
    ))
}

fn period_dropdown(active: Option<Period>) -> CreateContainerComponent<'static> {
    let active_key = match active {
        Some(Period::Monthly) => "monthly",
        Some(Period::Weekly) => "weekly",
        _ => "all",
    };
    let options = [
        ("all", "All-time"),
        ("monthly", "Monthly"),
        ("weekly", "Weekly"),
    ]
    .iter()
    .map(|(value, label)| {
        let mut option = CreateSelectMenuOption::new(*label, *value);
        if *value == active_key {
            option = option.default_selection(true);
        }
        option
    })
    .collect::<Vec<_>>();
    CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(CreateSelectMenu::new(
        "info_taggers",
        CreateSelectMenuKind::String {
            options: options.into(),
        },
    )))
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
