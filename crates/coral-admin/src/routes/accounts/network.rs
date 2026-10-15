use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;
use sqlx::FromRow;

use super::AccountTarget;
use crate::error::{AdminError, Context};
use crate::refs::{EntityRef, Labels};
use crate::routes::ref_labels::label_request;
use crate::serde_ts;
use crate::state::AppState;

const LISTED_SHARERS: i64 = 50;
const IMPORT_FINISHED_ON: (i32, u32, u32) = (2026, 3, 27);

#[derive(Serialize)]
pub struct AccountNetwork {
    ips: Vec<IpUse>,
    agents: Vec<AgentUse>,
    countries: Vec<CountryUse>,
    linked_accounts: Vec<LinkedAccount>,
    labels: Labels,
}

#[derive(Serialize)]
pub struct IpUse {
    pub ip: EntityRef,
    pub requests: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub first_seen: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub last_seen: DateTime<Utc>,
    pub imported: bool,
    pub shared_with: Vec<EntityRef>,
    pub shared_count: i64,
}

#[derive(Serialize, FromRow)]
pub struct AgentUse {
    pub user_agent: String,
    pub requests: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub first_seen: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub last_seen: DateTime<Utc>,
}

#[derive(Serialize, FromRow)]
struct CountryUse {
    country: Option<String>,
    requests: i64,
}

#[derive(Serialize)]
struct LinkedAccount {
    account: EntityRef,
    ips: Vec<EntityRef>,
}

#[derive(FromRow)]
struct IpRow {
    ip: String,
    requests: i64,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    country: Option<String>,
    proxied: bool,
    sharer_discord_ids: Vec<i64>,
    sharer_member_ids: Vec<i64>,
    shared_count: i64,
}

pub async fn network(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<AccountNetwork>, AdminError> {
    let target = AccountTarget::resolve(&state, &raw).await?;
    let Some(member_id) = target.member_id else {
        return Ok(Json(empty_network()));
    };
    let (ips, agents, countries) = tokio::try_join!(
        ip_rows(&state, member_id),
        agents(&state, member_id),
        countries(&state, member_id),
    )?;
    let ips: Vec<IpUse> = ips.into_iter().map(ip_use).collect();
    let linked_accounts = linked_accounts(&ips);
    let mentioned = linked_accounts.iter().map(|linked| &linked.account);
    let labels = state.names.cached(&label_request(mentioned)).await?;
    Ok(Json(AccountNetwork {
        ips,
        agents,
        countries,
        linked_accounts,
        labels,
    }))
}

pub fn imported_before() -> DateTime<Utc> {
    let (year, month, day) = IMPORT_FINISHED_ON;
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0)
        .single()
        .expect("the import date is a valid UTC date")
}

pub fn is_imported(requests: i64, last_seen: DateTime<Utc>) -> bool {
    requests == 0 && last_seen < imported_before()
}

async fn ip_rows(state: &AppState, member_id: i64) -> Result<Vec<IpRow>, AdminError> {
    sqlx::query_as(
        "SELECT host(i.ip_address) AS ip, i.requests, i.first_seen, i.last_seen, i.country, i.proxied,
                coalesce(sharers.discord_ids, '{}') AS sharer_discord_ids,
                coalesce(sharers.member_ids, '{}') AS sharer_member_ids,
                shared.count AS shared_count
         FROM api_key_ips i
         LEFT JOIN LATERAL (
            SELECT array_agg(listed.discord_id) AS discord_ids, array_agg(listed.id) AS member_ids
            FROM (
                SELECT m.discord_id, m.id FROM api_key_ips o JOIN members m ON m.id = o.member_id
                WHERE o.ip_address = i.ip_address AND o.member_id <> i.member_id
                ORDER BY o.last_seen DESC LIMIT $2
            ) AS listed
         ) AS sharers ON true
         LEFT JOIN LATERAL (
            SELECT count(*) AS count FROM api_key_ips o
            WHERE o.ip_address = i.ip_address AND o.member_id <> i.member_id
         ) AS shared ON true
         WHERE i.member_id = $1
         ORDER BY i.last_seen DESC, i.id DESC",
    )
    .bind(member_id)
    .bind(LISTED_SHARERS)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's IP addresses")
}

async fn agents(state: &AppState, member_id: i64) -> Result<Vec<AgentUse>, AdminError> {
    sqlx::query_as(
        "SELECT user_agent, requests, first_seen, last_seen FROM api_member_agents
         WHERE member_id = $1 ORDER BY last_seen DESC, requests DESC",
    )
    .bind(member_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's user agents")
}

async fn countries(state: &AppState, member_id: i64) -> Result<Vec<CountryUse>, AdminError> {
    sqlx::query_as(
        "SELECT country, sum(requests)::int8 AS requests FROM api_key_ips
         WHERE member_id = $1 GROUP BY country ORDER BY requests DESC, country",
    )
    .bind(member_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's countries")
}

fn ip_use(row: IpRow) -> IpUse {
    IpUse {
        imported: is_imported(row.requests, row.last_seen),
        ip: EntityRef::Ip {
            ip: row.ip,
            country: row.country,
            proxied: row.proxied,
        },
        requests: row.requests,
        first_seen: row.first_seen,
        last_seen: row.last_seen,
        shared_with: row
            .sharer_discord_ids
            .into_iter()
            .zip(row.sharer_member_ids)
            .map(|(discord_id, member_id)| EntityRef::Account {
                discord_id,
                member_id: Some(member_id),
            })
            .collect(),
        shared_count: row.shared_count,
    }
}

fn linked_accounts(ips: &[IpUse]) -> Vec<LinkedAccount> {
    let mut linked: BTreeMap<i64, (EntityRef, Vec<EntityRef>)> = BTreeMap::new();
    for used in ips {
        for sharer in &used.shared_with {
            let EntityRef::Account { discord_id, .. } = sharer else {
                continue;
            };
            linked
                .entry(*discord_id)
                .or_insert_with(|| (sharer.clone(), Vec::new()))
                .1
                .push(used.ip.clone());
        }
    }
    let mut accounts: Vec<LinkedAccount> = linked
        .into_values()
        .map(|(account, ips)| LinkedAccount { account, ips })
        .collect();
    accounts.sort_by(|a, b| b.ips.len().cmp(&a.ips.len()));
    accounts
}

fn empty_network() -> AccountNetwork {
    AccountNetwork {
        ips: Vec::new(),
        agents: Vec::new(),
        countries: Vec::new(),
        linked_accounts: Vec::new(),
        labels: Labels::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn used(ip: &str, sharers: &[i64]) -> IpUse {
        IpUse {
            ip: EntityRef::Ip {
                ip: ip.into(),
                country: None,
                proxied: false,
            },
            requests: 1,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            imported: false,
            shared_with: sharers
                .iter()
                .map(|discord_id| EntityRef::Account {
                    discord_id: *discord_id,
                    member_id: Some(1),
                })
                .collect(),
            shared_count: sharers.len() as i64,
        }
    }

    #[test]
    fn linked_accounts_list_every_shared_address_most_shared_first() {
        let linked = linked_accounts(&[used("10.0.0.1", &[7, 9]), used("10.0.0.2", &[9])]);
        assert_eq!(linked.len(), 2);
        assert!(matches!(
            linked[0].account,
            EntityRef::Account { discord_id: 9, .. }
        ));
        assert_eq!(linked[0].ips.len(), 2);
        assert_eq!(linked[1].ips.len(), 1);
    }

    #[test]
    fn only_untouched_rows_from_the_march_import_count_as_imported() {
        let march = Utc.with_ymd_and_hms(2026, 3, 26, 8, 16, 1).unwrap();
        assert!(is_imported(0, march));
        assert!(!is_imported(3, march));
        assert!(!is_imported(
            0,
            Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()
        ));
    }
}
