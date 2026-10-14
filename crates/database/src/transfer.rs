use std::collections::BTreeMap;

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::members::{COLUMNS as MEMBER_COLUMNS, Member};

const SNOWFLAKE_COLUMNS: &[(&str, &str)] = &[
    ("player_events", "author"),
    ("player_events", "displaced_author"),
    ("player_snapshots", "discord_id"),
    ("pending_tag_notices", "discord_id"),
    ("guild_config", "configured_by"),
    ("guild_sync_jobs", "created_by"),
    ("review_guide_config", "posted_by"),
    ("admin_actions", "actor"),
    ("member_strike", "struck_by"),
    ("member_strike", "revoked_by"),
    ("review", "resolved_by"),
    ("review", "voided_by"),
    ("review_evidence", "added_by"),
    ("evidence_post", "pending_by"),
];

const SNOWFLAKE_ARRAYS: &[(&str, &str)] = &[("player_events", "reviewed_by")];

const KEYED_TABLES: &[&str] = &["session_markers", "guild_subscriptions"];

const REVIEW_PARTICIPANT_COLUMNS: &[(&str, &str)] =
    &[("review", "submitter_id"), ("review_vote", "voter_id")];

const DISCARDED_TABLES: &[&str] = &["discord_username_cache"];

#[derive(Debug)]
pub enum TransferError {
    SameAccount,
    NothingToTransfer,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for TransferError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferPreview {
    pub tags_authored: i64,
    pub linked_accounts: i64,
}

#[derive(Debug, Default)]
pub struct TransferOutcome {
    pub counts: BTreeMap<String, u64>,
    pub dropped: Vec<&'static str>,
}

impl TransferOutcome {
    fn record(&mut self, key: impl Into<String>, rows: u64) {
        if rows > 0 {
            *self.counts.entry(key.into()).or_default() += rows;
        }
    }
}

pub struct TransferRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> TransferRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn preview(
        &self,
        source: i64,
        target: i64,
    ) -> Result<TransferPreview, TransferError> {
        if source == target {
            return Err(TransferError::SameAccount);
        }
        let mut conn = self.pool.acquire().await?;
        if !source_has_data(&mut conn, source).await? {
            return Err(TransferError::NothingToTransfer);
        }
        let (tags_authored,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM player_events WHERE author = $1 AND kind = 'tag_set'",
        )
        .bind(source)
        .fetch_one(&mut *conn)
        .await?;
        let (linked_accounts,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM minecraft_accounts a
             JOIN members m ON m.id = a.member_id WHERE m.discord_id = $1",
        )
        .bind(source)
        .fetch_one(&mut *conn)
        .await?;
        Ok(TransferPreview {
            tags_authored,
            linked_accounts,
        })
    }

    pub async fn transfer(
        &self,
        source: i64,
        target: i64,
        actor: i64,
    ) -> Result<TransferOutcome, TransferError> {
        if source == target {
            return Err(TransferError::SameAccount);
        }
        let mut tx = self.pool.begin().await?;
        if !source_has_data(&mut tx, source).await? {
            return Err(TransferError::NothingToTransfer);
        }

        let mut outcome = TransferOutcome::default();
        merge_members(&mut tx, source, target, &mut outcome).await?;
        rewrite_snowflakes(&mut tx, source, target, &mut outcome).await?;
        merge_keyed_tables(&mut tx, source, target, &mut outcome).await?;
        merge_review_participation(&mut tx, source, target, &mut outcome).await?;
        rewrite_strike_actors(&mut tx, source, target, &mut outcome).await?;
        merge_starfish(&mut tx, source, target, &mut outcome).await?;
        discard_source_rows(&mut tx, source, &mut outcome).await?;
        if outcome.counts.is_empty() {
            return Err(TransferError::NothingToTransfer);
        }
        log_transfer(&mut tx, actor, source, target, &outcome).await?;

        tx.commit().await?;
        Ok(outcome)
    }
}

async fn source_has_data(conn: &mut PgConnection, source: i64) -> Result<bool, sqlx::Error> {
    let mut checks = vec!["SELECT 1 FROM members WHERE discord_id = $1".to_string()];
    checks.extend(
        SNOWFLAKE_COLUMNS
            .iter()
            .map(|(table, column)| format!("SELECT 1 FROM {table} WHERE {column} = $1")),
    );
    checks.extend(
        SNOWFLAKE_ARRAYS
            .iter()
            .map(|(table, column)| format!("SELECT 1 FROM {table} WHERE $1 = ANY({column})")),
    );
    checks.extend(
        KEYED_TABLES
            .iter()
            .map(|table| format!("SELECT 1 FROM {table} WHERE discord_id = $1")),
    );
    checks.extend(
        REVIEW_PARTICIPANT_COLUMNS
            .iter()
            .map(|(table, column)| format!("SELECT 1 FROM {table} WHERE {column} = $1")),
    );
    checks.push("SELECT 1 FROM starfish_users WHERE discord_id = $1".to_string());

    let sql = format!(
        "SELECT EXISTS ({})",
        checks
            .iter()
            .map(|c| format!("({c} LIMIT 1)"))
            .collect::<Vec<_>>()
            .join(" UNION ALL ")
    );
    let (exists,): (bool,) = sqlx::query_as(&sql)
        .bind(source)
        .fetch_one(&mut *conn)
        .await?;
    Ok(exists)
}

async fn lock_member(
    conn: &mut PgConnection,
    discord_id: i64,
) -> Result<Option<Member>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {MEMBER_COLUMNS} FROM members WHERE discord_id = $1 FOR UPDATE"
    ))
    .bind(discord_id)
    .fetch_optional(&mut *conn)
    .await
}

async fn merge_members(
    conn: &mut PgConnection,
    source: i64,
    target: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    let source_row = lock_member(conn, source).await?;
    let target_row = lock_member(conn, target).await?;
    let Some(source_row) = source_row else {
        return Ok(());
    };
    let Some(target_row) = target_row else {
        let moved = sqlx::query("UPDATE members SET discord_id = $2 WHERE id = $1")
            .bind(source_row.id)
            .bind(target)
            .execute(&mut *conn)
            .await?;
        outcome.record("members", moved.rows_affected());
        return Ok(());
    };

    repoint_member_rows(conn, source_row.id, target_row.id, outcome).await?;

    sqlx::query("DELETE FROM members WHERE id = $1")
        .bind(source_row.id)
        .execute(&mut *conn)
        .await?;
    merge_member_fields(conn, &source_row, &target_row, source, target).await?;
    outcome.record("members", 1);

    if source_row.api_key.is_some() && target_row.api_key.is_some() {
        outcome.dropped.push("api_key");
    }
    if matches!((&source_row.uuid, &target_row.uuid), (Some(s), Some(t)) if s != t) {
        outcome.dropped.push("uuid");
    }
    Ok(())
}

async fn repoint_member_rows(
    conn: &mut PgConnection,
    source_id: i64,
    target_id: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    for (table, key) in [
        ("api_key_ips", "ip_address"),
        ("minecraft_accounts", "uuid"),
    ] {
        sqlx::query(&format!(
            "DELETE FROM {table} s WHERE s.member_id = $1
               AND EXISTS (SELECT 1 FROM {table} t WHERE t.member_id = $2 AND t.{key} = s.{key})"
        ))
        .bind(source_id)
        .bind(target_id)
        .execute(&mut *conn)
        .await?;
        let moved = sqlx::query(&format!(
            "UPDATE {table} SET member_id = $2 WHERE member_id = $1"
        ))
        .bind(source_id)
        .bind(target_id)
        .execute(&mut *conn)
        .await?;
        outcome.record(table, moved.rows_affected());
    }

    let moved = sqlx::query(
        "UPDATE developer_keys SET member_id = $2 WHERE member_id = $1
           AND NOT EXISTS (SELECT 1 FROM developer_keys WHERE member_id = $2)",
    )
    .bind(source_id)
    .bind(target_id)
    .execute(&mut *conn)
    .await?;
    outcome.record("developer_keys", moved.rows_affected());
    let (source_key_left,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM developer_keys WHERE member_id = $1)")
            .bind(source_id)
            .fetch_one(&mut *conn)
            .await?;
    if source_key_left {
        outcome.dropped.push("developer_key");
    }

    let moved = sqlx::query("UPDATE member_strike SET member_id = $2 WHERE member_id = $1")
        .bind(source_id)
        .bind(target_id)
        .execute(&mut *conn)
        .await?;
    outcome.record("member_strike", moved.rows_affected());

    let moved = sqlx::query("UPDATE api_request_log SET member_id = $2 WHERE member_id = $1")
        .bind(source_id)
        .bind(target_id)
        .execute(&mut *conn)
        .await?;
    outcome.record("api_request_log", moved.rows_affected());

    let moved = sqlx::query("UPDATE api_request_detail SET member_id = $2 WHERE member_id = $1")
        .bind(source_id)
        .bind(target_id)
        .execute(&mut *conn)
        .await?;
    outcome.record("api_request_detail", moved.rows_affected());
    Ok(())
}

async fn merge_member_fields(
    conn: &mut PgConnection,
    source_row: &Member,
    target_row: &Member,
    source: i64,
    target: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE members SET
             api_key = COALESCE($2, api_key),
             key_locked = CASE WHEN $2 IS NULL THEN key_locked ELSE $3 END,
             request_count = CASE WHEN $2 IS NULL THEN request_count ELSE $4 END,
             uuid = COALESCE(uuid, $5),
             accepted_tags = accepted_tags + $6,
             rejected_tags = rejected_tags + $7,
             accurate_verdicts = accurate_verdicts + $8,
             incorrect_verdicts = incorrect_verdicts + $9,
             bonus_verdicts = bonus_verdicts + $10,
             vote_granted = vote_granted OR $11,
             tag_granted = tag_granted OR $12,
             access_level = GREATEST(access_level, $13),
             tagging_disabled = tagging_disabled OR $14,
             config = $15::jsonb || config,
             join_date = LEAST(join_date, $16),
             strikes = strikes || $17::jsonb
         WHERE id = $1",
    )
    .bind(target_row.id)
    .bind(&source_row.api_key)
    .bind(source_row.key_locked)
    .bind(source_row.request_count)
    .bind(&source_row.uuid)
    .bind(source_row.accepted_tags)
    .bind(source_row.rejected_tags)
    .bind(source_row.accurate_verdicts)
    .bind(source_row.incorrect_verdicts)
    .bind(source_row.bonus_verdicts)
    .bind(source_row.vote_granted)
    .bind(source_row.tag_granted)
    .bind(source_row.access_level)
    .bind(source_row.tagging_disabled)
    .bind(&source_row.config)
    .bind(source_row.join_date)
    .bind(reattribute_strikes(&source_row.strikes, source, target))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

fn reattribute_strikes(strikes: &Value, source: i64, target: i64) -> Value {
    let entries = strikes.as_array().cloned().unwrap_or_default();
    Value::Array(
        entries
            .into_iter()
            .map(|mut entry| {
                if entry.get("struck_by").and_then(Value::as_i64) == Some(source) {
                    entry["struck_by"] = json!(target);
                }
                entry
            })
            .collect(),
    )
}

async fn rewrite_snowflakes(
    conn: &mut PgConnection,
    source: i64,
    target: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    for (table, column) in SNOWFLAKE_COLUMNS {
        let moved = sqlx::query(&format!(
            "UPDATE {table} SET {column} = $2 WHERE {column} = $1"
        ))
        .bind(source)
        .bind(target)
        .execute(&mut *conn)
        .await?;
        outcome.record(format!("{table}.{column}"), moved.rows_affected());
    }
    for (table, column) in SNOWFLAKE_ARRAYS {
        let moved = sqlx::query(&format!(
            "UPDATE {table} SET {column} = array_replace({column}, $1, $2) WHERE $1 = ANY({column})"
        ))
        .bind(source)
        .bind(target)
        .execute(&mut *conn)
        .await?;
        outcome.record(format!("{table}.{column}"), moved.rows_affected());
    }
    let moved = sqlx::query("UPDATE admin_actions SET target = $2 WHERE target = $1")
        .bind(source.to_string())
        .bind(target.to_string())
        .execute(&mut *conn)
        .await?;
    outcome.record("admin_actions.target", moved.rows_affected());
    Ok(())
}

async fn merge_keyed_tables(
    conn: &mut PgConnection,
    source: i64,
    target: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM session_markers s WHERE s.discord_id = $1
           AND EXISTS (SELECT 1 FROM session_markers t
                       WHERE t.discord_id = $2 AND t.uuid = s.uuid AND t.name = s.name)",
    )
    .bind(source)
    .bind(target)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE guild_subscriptions t SET tag_types = CASE
             WHEN cardinality(t.tag_types) = 0 OR cardinality(s.tag_types) = 0 THEN '{}'
             ELSE ARRAY(SELECT DISTINCT unnest(t.tag_types || s.tag_types))
         END
         FROM guild_subscriptions s
         WHERE t.discord_id = $2 AND s.discord_id = $1 AND s.guild_id = t.guild_id",
    )
    .bind(source)
    .bind(target)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM guild_subscriptions s WHERE s.discord_id = $1
           AND EXISTS (SELECT 1 FROM guild_subscriptions t
                       WHERE t.discord_id = $2 AND t.guild_id = s.guild_id)",
    )
    .bind(source)
    .bind(target)
    .execute(&mut *conn)
    .await?;

    for table in KEYED_TABLES {
        let moved = sqlx::query(&format!(
            "UPDATE {table} SET discord_id = $2 WHERE discord_id = $1"
        ))
        .bind(source)
        .bind(target)
        .execute(&mut *conn)
        .await?;
        outcome.record(*table, moved.rows_affected());
    }
    Ok(())
}

async fn merge_review_participation(
    conn: &mut PgConnection,
    source: i64,
    target: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM review s WHERE s.submitter_id = $1 AND s.status = 'draft'
           AND EXISTS (SELECT 1 FROM review t WHERE t.submitter_id = $2 AND t.status = 'draft'
                       AND t.player_uuid = s.player_uuid)",
    )
    .bind(source)
    .bind(target)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE review_vote s SET disposition = 'superseded'
         WHERE s.voter_id = $1 AND s.disposition = 'active'
           AND EXISTS (SELECT 1 FROM review_vote t WHERE t.voter_id = $2
                       AND t.disposition = 'active' AND t.review_id = s.review_id)",
    )
    .bind(source)
    .bind(target)
    .execute(&mut *conn)
    .await?;
    for (table, column) in REVIEW_PARTICIPANT_COLUMNS {
        let moved = sqlx::query(&format!(
            "UPDATE {table} SET {column} = $2 WHERE {column} = $1"
        ))
        .bind(source)
        .bind(target)
        .execute(&mut *conn)
        .await?;
        outcome.record(format!("{table}.{column}"), moved.rows_affected());
    }
    Ok(())
}

async fn rewrite_strike_actors(
    conn: &mut PgConnection,
    source: i64,
    target: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    let moved = sqlx::query(
        "UPDATE members SET strikes = (
             SELECT jsonb_agg(
                 CASE WHEN (e ->> 'struck_by')::bigint = $1
                      THEN jsonb_set(e, '{struck_by}', to_jsonb($2::bigint))
                      ELSE e END
                 ORDER BY ord)
             FROM jsonb_array_elements(strikes) WITH ORDINALITY AS s(e, ord))
         WHERE strikes @> jsonb_build_array(jsonb_build_object('struck_by', $1::bigint))",
    )
    .bind(source)
    .bind(target)
    .execute(&mut *conn)
    .await?;
    outcome.record("members.strikes.struck_by", moved.rows_affected());
    Ok(())
}

async fn merge_starfish(
    conn: &mut PgConnection,
    source: i64,
    target: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    let source_user: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM starfish_users WHERE discord_id = $1 FOR UPDATE")
            .bind(source)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((source_user,)) = source_user else {
        return Ok(());
    };
    let target_user: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM starfish_users WHERE discord_id = $1 FOR UPDATE")
            .bind(target)
            .fetch_optional(&mut *conn)
            .await?;

    let Some((target_user,)) = target_user else {
        sqlx::query("UPDATE starfish_users SET discord_id = $2 WHERE id = $1")
            .bind(source_user)
            .bind(target)
            .execute(&mut *conn)
            .await?;
        outcome.record("starfish_users", 1);
        return Ok(());
    };

    let moved = sqlx::query("UPDATE plugins SET owner_user_id = $2 WHERE owner_user_id = $1")
        .bind(source_user)
        .bind(target_user)
        .execute(&mut *conn)
        .await?;
    outcome.record("plugins.owner_user_id", moved.rows_affected());
    sqlx::query("DELETE FROM starfish_users WHERE id = $1")
        .bind(source_user)
        .execute(&mut *conn)
        .await?;
    outcome.dropped.push("starfish_account");
    Ok(())
}

async fn discard_source_rows(
    conn: &mut PgConnection,
    source: i64,
    outcome: &mut TransferOutcome,
) -> Result<(), sqlx::Error> {
    for table in DISCARDED_TABLES {
        let removed = sqlx::query(&format!("DELETE FROM {table} WHERE discord_id = $1"))
            .bind(source)
            .execute(&mut *conn)
            .await?;
        outcome.record(format!("{table} (deleted)"), removed.rows_affected());
    }
    let removed = sqlx::query(
        "DELETE FROM flag_dismissals
         WHERE flag_key IN ('budget:' || $1, 'probe:' || $1) OR flag_key LIKE 'spike:' || $1 || ':%'",
    )
    .bind(source.to_string())
    .execute(&mut *conn)
    .await?;
    outcome.record("flag_dismissals (deleted)", removed.rows_affected());
    Ok(())
}

async fn log_transfer(
    conn: &mut PgConnection,
    actor: i64,
    source: i64,
    target: i64,
    outcome: &TransferOutcome,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO admin_actions (actor, action, target, details)
         VALUES ($1, 'transfer_member', $2, $3)",
    )
    .bind(actor)
    .bind(target.to_string())
    .bind(json!({
        "source": source.to_string(),
        "counts": outcome.counts,
        "dropped": outcome.dropped,
    }))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemberRepository;

    #[test]
    fn reattributes_only_the_source_strikes() {
        let strikes = json!([
            {"reason": "a", "struck_by": 1, "timestamp": 0},
            {"reason": "b", "struck_by": 2, "timestamp": 0},
        ]);
        let moved = reattribute_strikes(&strikes, 1, 9);
        assert_eq!(moved[0]["struck_by"], json!(9));
        assert_eq!(moved[1]["struck_by"], json!(2));
    }

    #[test]
    fn reattributing_a_missing_array_yields_an_empty_one() {
        assert_eq!(reattribute_strikes(&Value::Null, 1, 9), json!([]));
    }

    async fn test_pool() -> Option<PgPool> {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .ok()
    }

    fn unique_ids() -> (i64, i64) {
        let base = chrono::Utc::now().timestamp_nanos_opt().unwrap() % 1_000_000_000_000;
        (
            900_000_000_000_000_000 + base * 2,
            900_000_000_000_000_000 + base * 2 + 1,
        )
    }

    async fn insert_member(pool: &PgPool, discord_id: i64, api_key: &str, uuid: &str) -> i64 {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO members (discord_id, api_key, uuid, accepted_tags, request_count, strikes)
             VALUES ($1, $2, $3, 2, 10, $4) RETURNING id",
        )
        .bind(discord_id)
        .bind(api_key)
        .bind(uuid)
        .bind(
            json!([{"reason": "r", "struck_by": discord_id, "timestamp": "2026-01-01T00:00:00Z"}]),
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO minecraft_accounts (member_id, uuid) VALUES ($1, 'shared'), ($1, $2)",
        )
        .bind(id)
        .bind(format!("own{discord_id}"))
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn cleanup(pool: &PgPool, ids: [i64; 2]) {
        for (table, column) in [
            ("members", "discord_id"),
            ("player_events", "author"),
            ("guild_subscriptions", "discord_id"),
            ("session_markers", "discord_id"),
            ("starfish_users", "discord_id"),
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE {column} = ANY($1)"))
                .bind(&ids[..])
                .execute(pool)
                .await
                .ok();
        }
        sqlx::query("DELETE FROM admin_actions WHERE target = ANY($1)")
            .bind(ids.iter().map(i64::to_string).collect::<Vec<_>>())
            .execute(pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn merges_source_into_existing_target() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (source, target) = unique_ids();
        cleanup(&pool, [source, target]).await;
        let source_key = format!("src-{source}");
        insert_member(&pool, source, &source_key, "srcuuid").await;
        let target_member = insert_member(&pool, target, &format!("tgt-{target}"), "tgtuuid").await;

        sqlx::query("INSERT INTO player_events (uuid, kind, tag_type, reason, author, reviewed_by) VALUES ('p', 'tag_set', 'closet_cheater', 'x', $1, ARRAY[$1, 5])")
            .bind(source)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO guild_subscriptions (guild_id, discord_id, tag_types) VALUES ('g1', $1, '{a}'), ('g1', $2, '{}'), ('g2', $1, '{b}'), ('g2', $2, '{c}')")
            .bind(source)
            .bind(target)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO session_markers (uuid, discord_id, name, snapshot_timestamp) VALUES ('u', $1, 'm', NOW()), ('u', $2, 'm', NOW()), ('u', $1, 'other', NOW())")
            .bind(source)
            .bind(target)
            .execute(&pool)
            .await
            .unwrap();

        let outcome = TransferRepository::new(&pool)
            .transfer(source, target, 1)
            .await
            .unwrap();
        assert_eq!(outcome.dropped, vec!["api_key", "uuid"]);

        let (remaining,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM members WHERE discord_id = ANY($1)")
                .bind(&[source, target][..])
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 1);
        let merged = MemberRepository::new(&pool)
            .get_by_discord_id(target)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(merged.id, target_member);
        assert_eq!(merged.api_key.as_deref(), Some(source_key.as_str()));
        assert_eq!(merged.uuid.as_deref(), Some("tgtuuid"));
        assert_eq!(merged.accepted_tags, 4);
        let strikes = &merged.strikes;
        let actors: Vec<i64> = strikes
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["struck_by"].as_i64().unwrap())
            .collect();
        assert_eq!(actors, vec![target, target]);

        let (accounts,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM minecraft_accounts WHERE member_id = $1")
                .bind(target_member)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(accounts, 3);

        let (author, reviewers): (i64, Vec<i64>) = sqlx::query_as(
            "SELECT author, reviewed_by FROM player_events WHERE author = $1 AND uuid = 'p'",
        )
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((author, reviewers), (target, vec![target, 5]));

        let subs: Vec<(String, Vec<String>)> = sqlx::query_as(
            "SELECT guild_id, tag_types FROM guild_subscriptions WHERE discord_id = $1 ORDER BY guild_id",
        )
        .bind(target)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(subs.len(), 2);
        assert!(subs[0].1.is_empty());
        let mut merged = subs[1].1.clone();
        merged.sort();
        assert_eq!(merged, vec!["b", "c"]);

        let (markers,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM session_markers WHERE discord_id = $1")
                .bind(target)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(markers, 2);

        let (logged,): (Value,) = sqlx::query_as(
            "SELECT details FROM admin_actions WHERE action = 'transfer_member' AND target = $1",
        )
        .bind(target.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(logged["source"], json!(source.to_string()));

        cleanup(&pool, [source, target]).await;
    }

    #[tokio::test]
    async fn moves_source_row_when_target_is_unregistered() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (source, target) = unique_ids();
        cleanup(&pool, [source, target]).await;
        let source_member = insert_member(&pool, source, &format!("src-{source}"), "srcuuid").await;

        let outcome = TransferRepository::new(&pool)
            .transfer(source, target, 1)
            .await
            .unwrap();
        assert!(outcome.dropped.is_empty());

        let (id,): (i64,) = sqlx::query_as("SELECT id FROM members WHERE discord_id = $1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(id, source_member);

        cleanup(&pool, [source, target]).await;
    }

    #[tokio::test]
    async fn refuses_same_account_and_empty_source() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (source, target) = unique_ids();
        let repo = TransferRepository::new(&pool);
        assert!(matches!(
            repo.preview(source, source).await,
            Err(TransferError::SameAccount)
        ));
        assert!(matches!(
            repo.transfer(source, target, 1).await,
            Err(TransferError::NothingToTransfer)
        ));
    }
}
