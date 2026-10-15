use std::net::IpAddr;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::query::RequestFilter;
use crate::refs::{EntityRef, Labels, Page, RequestRow};
use crate::routes::accounts::network::{AgentUse, is_imported};
use crate::routes::ref_labels::label_request;
use crate::routes::streams::{CF_WORKER_HEADER, StreamScope, request_stream};
use crate::serde_ts;
use crate::state::AppState;

const DAY: TimeDelta = TimeDelta::hours(24);
const AGENT_SAMPLE: i64 = 1_000;
const RECENT_REQUESTS_CAP: i64 = 10_000;

#[derive(Serialize)]
pub struct IpCore {
    ip: String,
    country: Option<String>,
    proxied: bool,
    accounts: Vec<IpAccount>,
    agents: Vec<AgentUse>,
    agents_sample: i64,
    requests_24h: i64,
    requests_24h_capped: bool,
    labels: Labels,
}

#[derive(Serialize)]
struct IpAccount {
    account: EntityRef,
    requests: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    first_seen: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    last_seen: DateTime<Utc>,
    imported: bool,
}

#[derive(FromRow)]
struct AccountRow {
    discord_id: i64,
    member_id: i64,
    requests: i64,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    country: Option<String>,
    proxied: bool,
}

#[derive(FromRow)]
struct NewestRequest {
    ts: DateTime<Utc>,
    request_id: Option<Uuid>,
    country: Option<String>,
}

#[derive(FromRow)]
struct AgentGroup {
    user_agent: Option<String>,
    requests: i64,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

struct AgentSample {
    agents: Vec<AgentUse>,
    sampled: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{ip}", get(ip))
        .route("/{ip}/requests", get(requests))
}

async fn ip(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<IpCore>, AdminError> {
    let ip = parse_ip(&raw)?;
    let (accounts, newest, agents, requests_24h) = tokio::try_join!(
        accounts(&state, ip),
        newest_request(&state, ip),
        agents(&state, ip),
        recent_requests(&state, ip),
    )?;
    let worker = came_through_worker(&state, newest.as_ref()).await?;
    let newest_account = accounts.iter().max_by_key(|row| row.last_seen);
    let country = newest
        .and_then(|row| row.country)
        .or_else(|| newest_account.and_then(|row| row.country.clone()));
    let proxied = worker || accounts.iter().any(|row| row.proxied);
    let accounts: Vec<IpAccount> = accounts.into_iter().map(ip_account).collect();
    let labels = state
        .names
        .cached(&label_request(accounts.iter().map(|used| &used.account)))
        .await?;
    Ok(Json(IpCore {
        ip: ip.to_string(),
        country,
        proxied,
        accounts,
        agents: agents.agents,
        agents_sample: agents.sampled,
        requests_24h,
        requests_24h_capped: requests_24h >= RECENT_REQUESTS_CAP,
        labels,
    }))
}

async fn requests(
    State(state): State<AppState>,
    Path(raw): Path<String>,
    filter: Result<Query<RequestFilter>, QueryRejection>,
) -> Result<Json<Page<RequestRow>>, AdminError> {
    let Query(filter) = filter?;
    let ip = parse_ip(&raw)?;
    let scope = StreamScope::Ip(ip.to_string());
    Ok(Json(request_stream(&state, scope, filter).await?))
}

fn parse_ip(raw: &str) -> Result<IpAddr, AdminError> {
    raw.parse()
        .map_err(|_| AdminError::BadRequest(format!("{raw:?} is not an IP address")))
}

async fn accounts(state: &AppState, ip: IpAddr) -> Result<Vec<AccountRow>, AdminError> {
    sqlx::query_as(
        "SELECT m.discord_id, m.id AS member_id, i.requests, i.first_seen, i.last_seen, i.country, i.proxied
         FROM api_key_ips i JOIN members m ON m.id = i.member_id
         WHERE i.ip_address = $1::inet
         ORDER BY i.last_seen DESC, i.requests DESC",
    )
    .bind(ip.to_string())
    .fetch_all(state.db.pool())
    .await
    .context("read the accounts seen on this IP")
}

async fn newest_request(state: &AppState, ip: IpAddr) -> Result<Option<NewestRequest>, AdminError> {
    sqlx::query_as(
        "SELECT ts, request_id, country FROM api_request_log WHERE ip = $1 ORDER BY ts DESC LIMIT 1",
    )
    .bind(ip.to_string())
    .fetch_optional(state.db.pool())
    .await
    .context("read this IP's newest request")
}

async fn agents(state: &AppState, ip: IpAddr) -> Result<AgentSample, AdminError> {
    let groups: Vec<AgentGroup> = sqlx::query_as(
        "WITH sample AS (
            SELECT user_agent, ts FROM api_request_log WHERE ip = $1 ORDER BY ts DESC LIMIT $2
         )
         SELECT user_agent, count(*)::int8 AS requests, min(ts) AS first_seen, max(ts) AS last_seen
         FROM sample GROUP BY user_agent
         ORDER BY last_seen DESC, requests DESC",
    )
    .bind(ip.to_string())
    .bind(AGENT_SAMPLE)
    .fetch_all(state.db.pool())
    .await
    .context("read the user agents seen on this IP")?;
    Ok(agent_sample(groups))
}

async fn recent_requests(state: &AppState, ip: IpAddr) -> Result<i64, AdminError> {
    sqlx::query_scalar(
        "SELECT count(*) FROM (
            SELECT 1 FROM api_request_log WHERE ip = $1 AND ts >= $2 LIMIT $3
         ) AS recent",
    )
    .bind(ip.to_string())
    .bind(Utc::now() - DAY)
    .bind(RECENT_REQUESTS_CAP)
    .fetch_one(state.db.pool())
    .await
    .context("count this IP's requests in the last 24 hours")
}

async fn came_through_worker(
    state: &AppState,
    newest: Option<&NewestRequest>,
) -> Result<bool, AdminError> {
    let Some((ts, request_id)) = newest.and_then(|row| Some((row.ts, row.request_id?))) else {
        return Ok(false);
    };
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM api_request_detail WHERE ts = $1 AND request_id = $2 AND headers ? $3
         )",
    )
    .bind(ts)
    .bind(request_id)
    .bind(CF_WORKER_HEADER)
    .fetch_one(state.db.pool())
    .await
    .context("check whether this IP's newest request came through a Cloudflare Worker")
}

fn agent_sample(groups: Vec<AgentGroup>) -> AgentSample {
    let sampled = groups.iter().map(|group| group.requests).sum();
    let agents = groups
        .into_iter()
        .filter_map(|group| {
            Some(AgentUse {
                user_agent: group.user_agent?,
                requests: group.requests,
                first_seen: group.first_seen,
                last_seen: group.last_seen,
            })
        })
        .collect();
    AgentSample { agents, sampled }
}

fn ip_account(row: AccountRow) -> IpAccount {
    IpAccount {
        imported: is_imported(row.requests, row.last_seen),
        account: EntityRef::Account {
            discord_id: row.discord_id,
            member_id: Some(row.member_id),
        },
        requests: row.requests,
        first_seen: row.first_seen,
        last_seen: row.last_seen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_without_a_user_agent_count_toward_the_sample_only() {
        let at = Utc::now();
        let group = |user_agent: Option<&str>, requests| AgentGroup {
            user_agent: user_agent.map(str::to_owned),
            requests,
            first_seen: at,
            last_seen: at,
        };
        let sample = agent_sample(vec![group(Some("client/1"), 7), group(None, 3)]);
        assert_eq!(sample.sampled, 10);
        assert_eq!(sample.agents.len(), 1);
        assert_eq!(sample.agents[0].user_agent, "client/1");
    }

    #[test]
    fn only_addresses_open_an_ip_page() {
        assert_eq!(parse_ip("203.0.113.9").unwrap().to_string(), "203.0.113.9");
        assert_eq!(parse_ip("2001:DB8::1").unwrap().to_string(), "2001:db8::1");
        for raw in ["unknown", "203.0.113", "203.0.113.9/24", ""] {
            assert_eq!(parse_ip(raw).unwrap_err().code(), "bad_request", "{raw}");
        }
    }
}
