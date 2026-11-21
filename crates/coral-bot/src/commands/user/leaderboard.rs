use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::Utc;
use serenity::all::*;

use database::{Leaderboard, LeaderboardRepository, LeaderboardRow, Period};

use crate::{
    framework::Data,
    utils::{format_number, separator, text},
};

pub const BOARD_SELECT: &str = "lb_board:";
pub const PERIOD_SELECT: &str = "lb_period:";

const LIMIT: i64 = 10;
const CACHE_TTL: Duration = Duration::from_secs(300);
const PERIODS: [Option<Period>; 3] = [None, Some(Period::Monthly), Some(Period::Weekly)];

type Page = Vec<CreateComponent<'static>>;
pub type LeaderboardCache =
    std::collections::HashMap<(Leaderboard, Option<Period>), (Page, Instant)>;

pub fn register() -> CreateCommand<'static> {
    let category = Leaderboard::ALL.into_iter().fold(
        CreateCommandOption::new(
            CommandOptionType::String,
            "category",
            "Which leaderboard to show",
        ),
        |option, board| option.add_string_choice(board.label(), board.key()),
    );
    CreateCommand::new("leaderboard")
        .description("Top taggers, voters, reviewers, and requesters")
        .add_option(category)
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;
    let board = command
        .data
        .options
        .iter()
        .find(|option| option.name == "category")
        .and_then(|option| option.value.as_str())
        .and_then(Leaderboard::from_key)
        .unwrap_or(Leaderboard::Taggers);
    command
        .edit_response(&ctx.http, page_response(page(data, board, None).await))
        .await?;
    Ok(())
}

pub async fn handle_select(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let ComponentInteractionDataKind::StringSelect { values } = &component.data.kind else {
        return Ok(());
    };
    let selected = values.first().map(String::as_str).unwrap_or_default();
    let id = component.data.custom_id.as_str();

    // Each select's custom id carries the other select's current value.
    let (board, period) = if let Some(period) = id.strip_prefix(BOARD_SELECT) {
        (Leaderboard::from_key(selected), Period::from_str(period))
    } else if let Some(board) = id.strip_prefix(PERIOD_SELECT) {
        (Leaderboard::from_key(board), Period::from_str(selected))
    } else {
        return Ok(());
    };
    let board = board.unwrap_or(Leaderboard::Taggers);
    let period = period.filter(|_| board.supports_periods());

    component
        .edit_response(&ctx.http, page_response(page(data, board, period).await))
        .await?;
    Ok(())
}

async fn page(data: &Data, board: Leaderboard, period: Option<Period>) -> Page {
    let key = (board, period);
    if let Some((page, at)) = data.leaderboard_cache.lock().unwrap().get(&key)
        && at.elapsed() < CACHE_TTL
    {
        return page.clone();
    }
    let page = build(data, board, period).await;
    data.leaderboard_cache
        .lock()
        .unwrap()
        .insert(key, (page.clone(), Instant::now()));
    page
}

async fn build(data: &Data, board: Leaderboard, period: Option<Period>) -> Page {
    let since = period.map(|p| p.last_reset(Utc::now()));
    let rows = LeaderboardRepository::new(data.db.pool())
        .top(board, since, LIMIT)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("leaderboard {} query failed: {e}", board.key());
            Vec::new()
        });

    let subtitle = match (period, since) {
        (Some(p), Some(since)) => format!("{} · since <t:{}:D>", p.label(), since.timestamp()),
        _ => "All-time".into(),
    };
    let mut parts = vec![
        text(format!(
            "## {}\n-# {subtitle} · updated <t:{}:R>",
            board.label(),
            Utc::now().timestamp()
        )),
        separator(),
        text(format_rows(&rows)),
        separator(),
        board_select(board, period),
    ];
    if board.supports_periods() {
        parts.push(period_select(board, period));
    }
    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

fn format_rows(rows: &[LeaderboardRow]) -> String {
    if rows.is_empty() {
        return "-# No data yet".into();
    }
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let name = row
                .username
                .as_deref()
                .map(|name| format!(" `{name}`"))
                .unwrap_or_default();
            format!(
                "`#{}` **{}** · <@{}>{name}",
                i + 1,
                format_number(row.total as u64),
                row.discord_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn period_key(period: Option<Period>) -> &'static str {
    period.map(|p| p.key()).unwrap_or("all")
}

fn board_select(active: Leaderboard, period: Option<Period>) -> CreateContainerComponent<'static> {
    let options = Leaderboard::ALL
        .into_iter()
        .map(|board| {
            CreateSelectMenuOption::new(board.label(), board.key())
                .default_selection(board == active)
        })
        .collect::<Vec<_>>();
    select(format!("{BOARD_SELECT}{}", period_key(period)), options)
}

fn period_select(board: Leaderboard, active: Option<Period>) -> CreateContainerComponent<'static> {
    let now = Utc::now();
    let options = PERIODS
        .into_iter()
        .map(|period| {
            let label = period.map_or("All-time".into(), |p| p.dropdown_label(now));
            CreateSelectMenuOption::new(label, period_key(period))
                .default_selection(period == active)
        })
        .collect::<Vec<_>>();
    select(format!("{PERIOD_SELECT}{}", board.key()), options)
}

fn select(
    custom_id: String,
    options: Vec<CreateSelectMenuOption<'static>>,
) -> CreateContainerComponent<'static> {
    CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(CreateSelectMenu::new(
        custom_id,
        CreateSelectMenuKind::String {
            options: options.into(),
        },
    )))
}

fn page_response(components: Page) -> EditInteractionResponse<'static> {
    EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .allowed_mentions(CreateAllowedMentions::new())
        .components(components)
}
