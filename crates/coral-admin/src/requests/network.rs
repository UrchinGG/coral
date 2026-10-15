use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use database::Database;
use sqlx::FromRow;
use uuid::Uuid;

use crate::coverage::Coverage;
use crate::error::{AdminError, Context};
use crate::query::LogRow;

pub const PROXY_HEADER: &str = "cf-worker";
pub const COUNTRY_HEADER: &str = "cf-ipcountry";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientNetwork {
    pub proxied: bool,
    pub country: Option<String>,
}

#[derive(Debug, Default)]
pub struct ClientNetworks {
    captured: HashMap<(DateTime<Utc>, Uuid), ClientNetwork>,
}

#[derive(FromRow)]
struct CapturedNetwork {
    ts: DateTime<Utc>,
    request_id: Uuid,
    proxied: bool,
    country: Option<String>,
}

impl ClientNetwork {
    pub fn from_headers(headers: &BTreeMap<String, String>) -> Self {
        Self {
            proxied: headers.contains_key(PROXY_HEADER),
            country: headers.get(COUNTRY_HEADER).cloned(),
        }
    }

    pub fn for_row(row: &LogRow, captured: Option<&ClientNetwork>) -> Self {
        Self {
            proxied: captured.is_some_and(|network| network.proxied),
            country: row
                .country
                .clone()
                .or_else(|| captured.and_then(|network| network.country.clone())),
        }
    }
}

impl ClientNetworks {
    pub async fn read<'a>(
        db: &Database,
        rows: impl IntoIterator<Item = &'a LogRow>,
        coverage: &Coverage,
    ) -> Result<Self, AdminError> {
        let captured: Vec<&LogRow> = rows
            .into_iter()
            .filter(|row| row.ip.is_some() && detail_was_kept(row, coverage))
            .collect();
        let (Some(earliest), Some(latest)) = (
            captured.iter().map(|row| row.ts).min(),
            captured.iter().map(|row| row.ts).max(),
        ) else {
            return Ok(Self::default());
        };
        let found: Vec<CapturedNetwork> = sqlx::query_as(
            "SELECT d.ts, d.request_id, d.headers ? $5 AS proxied, d.headers ->> $6 AS country
             FROM unnest($1::timestamptz[], $2::uuid[]) AS v(ts, request_id)
             CROSS JOIN LATERAL (
               SELECT d.ts, d.request_id, d.headers FROM api_request_detail d
               WHERE d.ts = v.ts AND d.request_id = v.request_id AND d.ts >= $3 AND d.ts <= $4
               LIMIT 1
             ) d",
        )
        .bind(captured.iter().map(|row| row.ts).collect::<Vec<_>>())
        .bind(
            captured
                .iter()
                .map(|row| row.request_key)
                .collect::<Vec<_>>(),
        )
        .bind(earliest)
        .bind(latest)
        .bind(PROXY_HEADER)
        .bind(COUNTRY_HEADER)
        .fetch_all(db.pool())
        .await
        .context("read the client networks of the listed requests")?;
        Ok(Self {
            captured: found
                .into_iter()
                .map(|network| {
                    let key = (network.ts, network.request_id);
                    let value = ClientNetwork {
                        proxied: network.proxied,
                        country: network.country,
                    };
                    (key, value)
                })
                .collect(),
        })
    }

    pub fn of(&self, row: &LogRow) -> ClientNetwork {
        ClientNetwork::for_row(row, self.captured.get(&(row.ts, row.request_key)))
    }
}

fn detail_was_kept(row: &LogRow, coverage: &Coverage) -> bool {
    coverage.detail_since.is_some_and(|since| row.ts >= since)
}
