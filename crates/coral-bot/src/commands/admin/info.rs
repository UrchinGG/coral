use std::fmt::Display;
use std::hash::Hash;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use futures_util::future::OptionFuture;
use serenity::all::*;

use database::{
    BlacklistRepository, CacheRepository, GuildCacheRepository, GuildCurrentRepository,
    MemberRepository, Period,
};

use crate::{
    framework::Data,
    utils::{format_number, separator, text},
};

const LIMIT: i64 = 10;
const WINDOW_SIZE: u8 = 1;
const CACHE_TTL: Duration = Duration::from_secs(300);

type Page = Vec<CreateComponent<'static>>;
type CountRaw = (i64, i64, i64);
type RatioRaw = (i64, i64, i64, i64, i64);

pub struct InfoCache {
    statistics: moka::future::Cache<(), Page>,
    leaderboard: LeaderboardCache,
    usernames: moka::future::Cache<u64, String>,
}

impl InfoCache {
    pub fn new() -> Self {
        Self {
            statistics: moka::future::Cache::builder()
                .max_capacity(1)
                .time_to_live(CACHE_TTL)
                .build(),
            leaderboard: LeaderboardCache::new(),
            usernames: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(CACHE_TTL)
                .build(),
        }
    }
}

impl Default for InfoCache {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LeaderboardCache {
    requesters: moka::future::Cache<(), Vec<CountRaw>>,
    taggers: moka::future::Cache<Option<Period>, Vec<CountRaw>>,
    reviewers: moka::future::Cache<(), Vec<RatioRaw>>,
    user_requesters: moka::future::Cache<u64, Vec<CountRaw>>,
    user_taggers: moka::future::Cache<(u64, Option<Period>), Vec<CountRaw>>,
    user_reviewers: moka::future::Cache<u64, Vec<RatioRaw>>,
}

impl LeaderboardCache {
    pub fn new() -> Self {
        Self {
            requesters: moka::future::Cache::builder()
                .max_capacity(1)
                .time_to_live(CACHE_TTL)
                .build(),
            taggers: moka::future::Cache::builder()
                .max_capacity(5)
                .time_to_live(CACHE_TTL)
                .build(),
            reviewers: moka::future::Cache::builder()
                .max_capacity(1)
                .time_to_live(CACHE_TTL)
                .build(),
            user_requesters: moka::future::Cache::builder()
                .max_capacity(1000)
                .time_to_live(CACHE_TTL)
                .build(),
            user_taggers: moka::future::Cache::builder()
                .max_capacity(1000)
                .time_to_live(CACHE_TTL)
                .build(),
            user_reviewers: moka::future::Cache::builder()
                .max_capacity(1000)
                .time_to_live(CACHE_TTL)
                .build(),
        }
    }
}

impl Default for LeaderboardCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderboardCategory {
    #[default]
    Blacklist,
    Reviews,
}

async fn cached<K, T, E>(
    cache: &moka::future::Cache<K, Vec<T>>,
    key: K,
    fetch: impl Future<Output = Result<Vec<T>, E>>,
) -> Vec<T>
where
    K: Hash + Eq + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
    E: Display + Send + Sync + 'static,
{
    cache.try_get_with(key, fetch).await.unwrap_or_else(|e| {
        tracing::warn!("leaderboard query failed: {e}");
        Vec::new()
    })
}

async fn statistics_page(ctx: &Context, data: &Data) -> Page {
    data.info_cache
        .statistics
        .get_with((), build_statistics(ctx, data))
        .await
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;
    command
        .edit_response(&ctx.http, page_response(statistics_page(ctx, data).await))
        .await?;

    let ctx = ctx.clone();
    let data = data.clone();
    let user = command.user.id.get();
    tokio::spawn(async move {
        // pre-populate leaderboards cache in the background
        build_leaderboards(
            &ctx,
            &data,
            LeaderboardCategory::default(),
            Some(user),
            None,
        )
        .await;
    });
    Ok(())
}

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("info").description("Urchin statistics and leaderboards")
}

pub async fn handle_page(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let user = component.user.id.get();
    let components = match component.data.custom_id.strip_prefix("info_page:") {
        Some("leaderboards") => {
            build_leaderboards(ctx, data, LeaderboardCategory::default(), Some(user), None).await
        }
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
    let user = component.user.id.get();
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
    let components = build_leaderboards(
        ctx,
        data,
        LeaderboardCategory::Blacklist,
        Some(user),
        period,
    )
    .await;
    component
        .edit_response(&ctx.http, page_response(components))
        .await?;
    Ok(())
}

pub async fn handle_leaderboard_category(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let user = component.user.id.get();
    let category = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => {
            match values.first().map(String::as_str) {
                Some("blacklist") => LeaderboardCategory::Blacklist,
                Some("reviews") => LeaderboardCategory::Reviews,
                _ => LeaderboardCategory::default(),
            }
        }
        _ => LeaderboardCategory::default(),
    };
    let components = build_leaderboards(ctx, data, category, Some(user), None).await;
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
        nav("statistics"),
    ];

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn build_leaderboards(
    ctx: &Context,
    data: &Data,
    category: LeaderboardCategory,
    user: Option<u64>,
    period: Option<Period>,
) -> Vec<CreateComponent<'static>> {
    let pool = data.db.pool();
    let cache = &data.info_cache.leaderboard;

    let mut parts = vec![
        text("## Leaderboards"),
        leaderboard_category_dropdown(category),
        separator(),
    ];
    parts.extend(match category {
        LeaderboardCategory::Blacklist => {
            let members = MemberRepository::new(pool);
            let blacklist = BlacklistRepository::new(pool);
            let since = period.map(|p| p.last_reset(Utc::now()));

            let (top_requesters, top_taggers) = tokio::join!(
                cached(&cache.requesters, (), members.top_requesters(LIMIT)),
                cached(&cache.taggers, period, blacklist.top_taggers(since, LIMIT))
            );
            // get nearby requesters and taggers for the user, iff. they are not already in the top N
            let (nearby_requesters, nearby_taggers) = tokio::join!(
                OptionFuture::from(
                    user.filter(|&u| !top_requesters.iter().any(|(_, id, _)| *id as u64 == u))
                        .map(|u| cached(
                            &cache.user_requesters,
                            u,
                            members.nearby_requesters(u as i64, WINDOW_SIZE),
                        ))
                ),
                OptionFuture::from(
                    user.filter(|&u| !top_taggers.iter().any(|(_, id, _)| *id as u64 == u))
                        .map(|u| cached(
                            &cache.user_taggers,
                            (u, period),
                            blacklist.nearby_taggers(since, u as i64, WINDOW_SIZE),
                        ))
                ),
            );

            vec![
                text(format!(
                    "### Top Requesters (All-time)\n{}{}",
                    leaderboard(ctx, data, &counts(top_requesters)).await,
                    nearby_section(
                        nearby_requesters,
                        |&(rank, _, _)| rank,
                        async |n| leaderboard(ctx, data, &counts(n)).await
                    )
                    .await
                )),
                separator(),
                text("### Top Taggers"),
                period_dropdown(period),
                text(format!(
                    "{}{}",
                    leaderboard(ctx, data, &counts(top_taggers)).await,
                    nearby_section(
                        nearby_taggers,
                        |&(rank, _, _)| rank,
                        async |n| leaderboard(ctx, data, &counts(n)).await
                    )
                    .await
                )),
            ]
        }
        LeaderboardCategory::Reviews => {
            let members = MemberRepository::new(pool);
            let reviewers = cached(&cache.reviewers, (), members.top_reviewers(LIMIT)).await;
            let nearby_reviewers = OptionFuture::from(
                user.filter(|&u| !reviewers.iter().any(|(_, id, _, _, _)| *id as u64 == u))
                    .map(|u| {
                        cached(
                            &cache.user_reviewers,
                            u,
                            members.nearby_reviewers(u as i64, WINDOW_SIZE),
                        )
                    }),
            )
            .await;

            vec![
                text("### Top Reviewers (All-time)"),
                text(format!(
                    "{}{}",
                    leaderboard(ctx, data, &counts_with_ratios(reviewers)).await,
                    nearby_section(
                        nearby_reviewers,
                        |&(rank, _, _, _, _)| rank,
                        async |n| leaderboard(ctx, data, &counts_with_ratios(n)).await,
                    )
                    .await,
                )),
            ]
        }
    });
    parts.push(nav("leaderboards"));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn leaderboard(ctx: &Context, data: &Data, rows: &[(i64, i64, String)]) -> String {
    if rows.is_empty() {
        return "-# No data yet".into();
    }
    let cache = &data.info_cache.usernames;
    let names = futures_util::future::join_all(rows.iter().map(|(_, id, _)| {
        let id = *id as u64;
        async move {
            cache
                .get_with(id, async {
                    ctx.http
                        .get_user(UserId::new(id))
                        .await
                        .map(|u| u.name.to_string())
                        .unwrap_or_else(|_| "unknown".into())
                })
                .await
        }
    }))
    .await;
    rows.iter()
        .zip(names)
        .map(|((rank, id, stat), name)| format!("{rank}. **{stat}** — <@{id}> `{name}`"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn counts(rows: Vec<CountRaw>) -> Vec<(i64, i64, String)> {
    rows.into_iter()
        .map(|(rank, id, count)| (rank, id, format_number(count as u64)))
        .collect()
}

fn counts_with_ratios(rows: Vec<RatioRaw>) -> Vec<(i64, i64, String)> {
    rows.into_iter()
        .map(|(rank, id, total, pos, neg)| {
            let (p, n) = (pos as u64, neg as u64);
            let ratio = match (p, n) {
                (0, 0) => "-".to_string(),
                (_, 0) => "∞".to_string(),
                _ => {
                    let h = (p * 100 + n / 2) / n; // hundredths, rounded
                    format!("{}.{:02}", format_number(h / 100), h % 100)
                }
            };
            (
                rank,
                id,
                format!("{} ({ratio})", format_number(total as u64)),
            )
        })
        .collect()
}

async fn nearby_section<T>(
    nearby: Option<Vec<T>>,
    rank: impl Fn(&T) -> i64,
    f: impl AsyncFnOnce(Vec<T>) -> String,
) -> String {
    let rows: Vec<T> = nearby
        .into_iter()
        .flatten()
        .filter(|i| rank(i) > LIMIT)
        .collect();
    let Some(first) = rows.first() else {
        return String::new();
    };
    let sep = if rank(first) == LIMIT + 1 {
        "\n"
    } else {
        "\n...\n"
    };
    format!("{sep}{}", f(rows).await)
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

fn leaderboard_category_dropdown(active: LeaderboardCategory) -> CreateContainerComponent<'static> {
    let active_key = match active {
        LeaderboardCategory::Blacklist => "blacklist",
        LeaderboardCategory::Reviews => "reviews",
    };
    let options = [("blacklist", "Blacklist"), ("reviews", "Reviews")]
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
        "info_leaderboard_category",
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
