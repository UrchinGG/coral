use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    CATCH_UP_CHUNK, NETWORK_FEATURE, NETWORK_WATERMARK, Span, log_floor, record_coverage,
    set_watermark,
};

pub const NETWORK_FOLD: &str = "
WITH rows AS (
  SELECT m.id AS member_id, l.ts,
         CASE WHEN pg_input_is_valid(l.ip, 'inet') THEN l.ip::inet END AS ip,
         coalesce(l.country, d.country) AS country, l.user_agent,
         coalesce(d.proxied, false) AS proxied
  FROM api_request_log l
  JOIN members m ON m.id = coalesce(
    l.member_id,
    CASE WHEN l.key_kind = 'session'
      THEN (SELECT s.id FROM members s WHERE s.discord_id = l.actor_id) END)
  LEFT JOIN LATERAL (
    SELECT d.headers ? 'cf-worker' AS proxied, d.headers ->> 'cf-ipcountry' AS country
    FROM api_request_detail d
    WHERE d.ts = l.ts AND d.request_id = l.request_id
    LIMIT 1
  ) d ON true
  WHERE l.ts >= $1 AND l.ts < $2 AND ($3::uuid[] IS NULL OR l.request_id = ANY($3))
),
ips AS (
  INSERT INTO api_key_ips AS t (member_id, ip_address, first_seen, last_seen, requests, country, proxied)
  SELECT member_id, ip, min(ts), max(ts), count(*),
         (array_agg(country ORDER BY ts DESC) FILTER (WHERE country IS NOT NULL))[1],
         bool_or(proxied)
  FROM rows WHERE ip IS NOT NULL
  GROUP BY member_id, ip
  ON CONFLICT (member_id, ip_address) DO UPDATE SET
    first_seen = least(t.first_seen, excluded.first_seen),
    last_seen = greatest(t.last_seen, excluded.last_seen),
    requests = t.requests + excluded.requests,
    country = coalesce(excluded.country, t.country),
    proxied = t.proxied OR excluded.proxied
)
INSERT INTO api_member_agents AS t (member_id, user_agent, first_seen, last_seen, requests)
SELECT member_id, user_agent, min(ts), max(ts), count(*)
FROM rows WHERE user_agent IS NOT NULL
GROUP BY member_id, user_agent
ON CONFLICT ON CONSTRAINT api_member_agents_key DO UPDATE SET
  first_seen = least(t.first_seen, excluded.first_seen),
  last_seen = greatest(t.last_seen, excluded.last_seen),
  requests = t.requests + excluded.requests
";

pub async fn advance_network(
    conn: &mut PgConnection,
    until: DateTime<Utc>,
) -> Result<Option<Span>, sqlx::Error> {
    let Some(from) = network_watermark(conn).await? else {
        return Ok(None);
    };
    let Some(span) = Span::new(from, (from + CATCH_UP_CHUNK).min(until)) else {
        return Ok(None);
    };
    sqlx::query(NETWORK_FOLD)
        .bind(span.from)
        .bind(span.to)
        .bind(None::<Vec<Uuid>>)
        .execute(&mut *conn)
        .await?;
    set_watermark(conn, NETWORK_WATERMARK, span.to).await?;
    Ok(Some(span))
}

async fn network_watermark(conn: &mut PgConnection) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    if let Some(watermark) = locked_network_watermark(conn).await? {
        return Ok(Some(watermark));
    }
    let Some(floor) = log_floor(conn).await? else {
        return Ok(None);
    };
    sqlx::query(
        "INSERT INTO api_rollup_state (name, watermark) VALUES ($1, $2)
         ON CONFLICT (name) DO NOTHING",
    )
    .bind(NETWORK_WATERMARK)
    .bind(floor)
    .execute(&mut *conn)
    .await?;
    record_coverage(conn, NETWORK_FEATURE, floor).await?;
    locked_network_watermark(conn).await
}

async fn locked_network_watermark(
    conn: &mut PgConnection,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar("SELECT watermark FROM api_rollup_state WHERE name = $1 FOR UPDATE")
        .bind(NETWORK_WATERMARK)
        .fetch_optional(conn)
        .await
}
