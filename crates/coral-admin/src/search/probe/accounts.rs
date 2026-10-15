use chrono::{DateTime, Utc};
use database::Database;
use sqlx::FromRow;

use super::{Account, DiscordSource, Found, ProbeContext, bounded, finish, open};
use crate::error::{AdminError, Context};
use crate::search::lookup::Reach;

#[derive(FromRow)]
struct AccountRow {
    member_id: Option<i64>,
    starfish: bool,
    cached: bool,
}

#[derive(FromRow)]
struct NamedRow {
    id: i64,
    member_id: Option<i64>,
    name: String,
}

pub(super) async fn account(db: &Database, discord_id: i64) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let row: AccountRow = sqlx::query_as(
        "SELECT (SELECT id FROM members WHERE discord_id = $1) AS member_id,
                EXISTS (SELECT 1 FROM starfish_users WHERE discord_id = $1) AS starfish,
                EXISTS (SELECT 1 FROM discord_username_cache WHERE discord_id = $1) AS cached",
    )
    .bind(discord_id)
    .fetch_one(&mut *tx)
    .await
    .context("look up the Discord id")?;
    finish(tx).await?;
    let found = match row {
        AccountRow {
            member_id: Some(member_id),
            ..
        } => Found::Member {
            account: Account::member(discord_id, member_id),
            by_number: false,
        },
        AccountRow { starfish: true, .. } => Found::DiscordUser {
            discord_id,
            source: DiscordSource::Starfish,
        },
        AccountRow { cached: true, .. } => Found::DiscordUser {
            discord_id,
            source: DiscordSource::NameCache,
        },
        _ => return Ok(Vec::new()),
    };
    Ok(vec![found])
}

pub(super) async fn bot_lookups(
    context: &ProbeContext<'_>,
    discord_id: i64,
) -> Result<Vec<Found>, AdminError> {
    let Some(since) = context.coverage.attribution_from else {
        return Ok(Vec::new());
    };
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let (last, member_id): (Option<DateTime<Utc>>, Option<i64>) = sqlx::query_as(
        "SELECT (SELECT ts FROM api_request_log
                 WHERE actor_id = $1 AND ts >= $2
                 ORDER BY ts DESC LIMIT 1),
                (SELECT id FROM members WHERE discord_id = $1)",
    )
    .bind(discord_id)
    .bind(since)
    .fetch_one(&mut *tx)
    .await
    .context("look for bot lookups run by the Discord id")?;
    finish(tx).await?;
    Ok(last
        .map(|last| Found::BotLookups {
            account: Account {
                discord_id,
                member_id,
            },
            last,
        })
        .into_iter()
        .collect())
}

pub(super) async fn member_number(db: &Database, number: i64) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let discord_id: Option<i64> =
        sqlx::query_scalar("SELECT discord_id FROM members WHERE id = $1")
            .bind(number)
            .fetch_optional(&mut *tx)
            .await
            .context("look up the member number")?;
    finish(tx).await?;
    Ok(discord_id
        .map(|discord_id| Found::Member {
            account: Account::member(discord_id, number),
            by_number: true,
        })
        .into_iter()
        .collect())
}

pub(super) async fn discord_names(
    context: &ProbeContext<'_>,
    text: &str,
    reach: Reach,
) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let rows: Vec<NamedRow> = sqlx::query_as(
        "SELECT c.discord_id AS id, m.id AS member_id, c.username AS name
         FROM discord_username_cache c
         LEFT JOIN members m ON m.discord_id = c.discord_id
         WHERE lower(c.username) = $1
            OR ($2 AND starts_with(lower(c.username), $1))
            OR ($3 AND strpos(lower(c.username), $1) > 0)
         ORDER BY lower(c.username) = $1 DESC, length(c.username), c.username
         LIMIT $4",
    )
    .bind(text)
    .bind(reach.prefix())
    .bind(reach.contains())
    .bind(context.match_limit)
    .fetch_all(&mut *tx)
    .await
    .context("look up Discord usernames")?;
    finish(tx).await?;
    Ok(rows
        .into_iter()
        .map(|row| Found::DiscordName {
            exact: row.name.to_lowercase() == text,
            account: Account {
                discord_id: row.id,
                member_id: row.member_id,
            },
            username: row.name,
        })
        .collect())
}
