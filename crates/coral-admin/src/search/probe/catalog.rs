use database::Database;
use sqlx::FromRow;

use super::{Found, ProbeContext, bounded, finish, open};
use crate::error::{AdminError, Context};
use crate::search::lookup::Reach;

#[derive(FromRow)]
struct LabelledRow {
    id: String,
    name: String,
}

pub(super) async fn plugins(
    context: &ProbeContext<'_>,
    text: &str,
    reach: Reach,
) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let rows: Vec<LabelledRow> = sqlx::query_as(
        "SELECT slug AS id, display_name AS name FROM plugins
         WHERE slug = $1 OR lower(display_name) = $1
            OR ($2 AND (starts_with(slug, $1) OR starts_with(lower(display_name), $1)))
            OR ($3 AND (strpos(slug, $1) > 0 OR strpos(lower(display_name), $1) > 0))
         ORDER BY slug = $1 DESC, slug
         LIMIT $4",
    )
    .bind(text)
    .bind(reach.prefix())
    .bind(reach.contains())
    .bind(context.match_limit)
    .fetch_all(&mut *tx)
    .await
    .context("look up plugins")?;
    finish(tx).await?;
    Ok(rows
        .into_iter()
        .map(|row| Found::Plugin {
            exact: row.id == text || row.name.to_lowercase() == text,
            slug: row.id,
            name: row.name,
        })
        .collect())
}

pub(super) async fn guild(db: &Database, guild_id: &str) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM guild_current WHERE guild_id = $1")
            .bind(guild_id)
            .fetch_optional(&mut *tx)
            .await
            .context("look up the guild")?;
    finish(tx).await?;
    Ok(name
        .map(|name| Found::Guild {
            guild_id: guild_id.to_string(),
            name,
            exact: true,
        })
        .into_iter()
        .collect())
}

pub(super) async fn guild_names(
    context: &ProbeContext<'_>,
    text: &str,
    reach: Reach,
) -> Result<Vec<Found>, AdminError> {
    let mut conn = open(context.db).await?;
    let mut tx = bounded(&mut conn).await?;
    let rows: Vec<LabelledRow> = sqlx::query_as(
        "SELECT guild_id AS id, name FROM guild_current
         WHERE lower(name) = $1
            OR ($2 AND starts_with(lower(name), $1))
            OR ($3 AND strpos(lower(name), $1) > 0)
         ORDER BY lower(name) = $1 DESC, member_count DESC
         LIMIT $4",
    )
    .bind(text)
    .bind(reach.prefix())
    .bind(reach.contains())
    .bind(context.match_limit)
    .fetch_all(&mut *tx)
    .await
    .context("look up guilds by name")?;
    finish(tx).await?;
    Ok(rows
        .into_iter()
        .map(|row| Found::Guild {
            exact: row.name.to_lowercase() == text,
            guild_id: row.id,
            name: row.name,
        })
        .collect())
}
