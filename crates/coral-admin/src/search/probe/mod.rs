use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use coral_redis::RedisPool;
use database::Database;
use reqlog::KeyKind;
use sqlx::pool::PoolConnection;
use sqlx::{Acquire, Postgres, Transaction};
use uuid::Uuid;

use super::lookup::{Lookup, Reach};
use crate::coverage::Coverage;
use crate::error::{AdminError, Context};
use crate::query::{acquire_within, bound_statements};

mod accounts;
mod catalog;
mod keys;
mod players;
mod traffic;

pub const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Account {
    pub discord_id: i64,
    pub member_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscordSource {
    Starfish,
    NameCache,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMatch {
    pub key_id: Option<i64>,
    pub prefix: String,
    pub kind: KeyKind,
    pub owner: Account,
    pub retired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sighting {
    Last(DateTime<Utc>),
    First(DateTime<Utc>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpTraffic {
    pub accounts: i64,
    pub requests: i64,
    pub country: Option<String>,
    pub proxied: bool,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Member {
        account: Account,
        by_number: bool,
    },
    DiscordUser {
        discord_id: i64,
        source: DiscordSource,
    },
    BotLookups {
        account: Account,
        last: DateTime<Utc>,
    },
    Player {
        uuid: Uuid,
        dashed: bool,
        owners: Vec<Account>,
    },
    PlayerRequests {
        uuid: Uuid,
        last: DateTime<Utc>,
    },
    Request {
        request_id: Uuid,
        ts: DateTime<Utc>,
        method: String,
        target: String,
        status: i16,
    },
    Key {
        key: KeyMatch,
        exact: bool,
    },
    UnknownPrefix {
        prefix: String,
        kind: KeyKind,
        last_hour: DateTime<Utc>,
    },
    Ip {
        ip: IpAddr,
        traffic: Option<IpTraffic>,
    },
    NameHolder {
        uuid: Uuid,
        name: String,
        current: bool,
        sighting: Option<Sighting>,
        owners: Vec<Account>,
    },
    NamedRequests {
        name: String,
        last: DateTime<Utc>,
    },
    DiscordName {
        account: Account,
        username: String,
        exact: bool,
    },
    Plugin {
        slug: String,
        name: String,
        exact: bool,
    },
    Guild {
        guild_id: String,
        name: String,
        exact: bool,
    },
    Merged {
        account: Account,
        username: String,
    },
}

pub struct ProbeContext<'a> {
    pub db: &'a Database,
    pub redis: &'a RedisPool,
    pub coverage: &'a Coverage,
    pub match_limit: i64,
}

impl Account {
    fn member(discord_id: i64, member_id: i64) -> Self {
        Self {
            discord_id,
            member_id: Some(member_id),
        }
    }
}

impl Reach {
    fn prefix(self) -> bool {
        matches!(self, Self::Prefix | Self::Contains)
    }

    fn contains(self) -> bool {
        self == Self::Contains
    }
}

impl Found {
    pub fn is_current_holder_of(&self, wanted: &str) -> bool {
        match self {
            Self::NameHolder { name, current, .. } => *current && name == wanted,
            _ => false,
        }
    }
}

pub async fn probe(lookup: &Lookup, context: &ProbeContext<'_>) -> Result<Vec<Found>, AdminError> {
    match lookup {
        Lookup::Account(id) => accounts::account(context.db, *id).await,
        Lookup::BotLookups(id) => accounts::bot_lookups(context, *id).await,
        Lookup::MemberNumber(number) => accounts::member_number(context.db, *number).await,
        Lookup::Player { uuid, dashed } => players::player(context.db, *uuid, *dashed).await,
        Lookup::PlayerRequests(uuid) => players::player_requests(context, *uuid).await,
        Lookup::Request(id) => traffic::request(context.db, *id).await,
        Lookup::ApiKey(key) => keys::api_key(context.db, *key).await,
        Lookup::Guild(id) => catalog::guild(context.db, id).await,
        Lookup::KeyPrefix(prefix) => keys::key_prefix(context, prefix).await,
        Lookup::Ip(ip) => traffic::ip_traffic(context.db, *ip).await,
        Lookup::NameHolders { name, history } => {
            players::name_holders(context, name, *history).await
        }
        Lookup::NamedRequests(name) => players::named_requests(context, name).await,
        Lookup::DiscordName(text, reach) => accounts::discord_names(context, text, *reach).await,
        Lookup::Plugin(text, reach) => catalog::plugins(context, text, *reach).await,
        Lookup::GuildName(text, reach) => catalog::guild_names(context, text, *reach).await,
    }
}

pub async fn player_owners(
    db: &Database,
    uuids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<Account>>, AdminError> {
    let mut conn = open(db).await?;
    let mut tx = bounded(&mut conn).await?;
    let owners = players::owners_of(&mut tx, uuids).await?;
    finish(tx).await?;
    Ok(owners)
}

async fn open(db: &Database) -> Result<PoolConnection<Postgres>, AdminError> {
    acquire_within(db.pool(), PROBE_TIMEOUT).await
}

async fn bounded(
    conn: &mut PoolConnection<Postgres>,
) -> Result<Transaction<'_, Postgres>, AdminError> {
    let started = Instant::now();
    let mut tx = conn.begin().await.context("begin a search probe")?;
    bound_statements(&mut tx, PROBE_TIMEOUT.saturating_sub(started.elapsed())).await?;
    Ok(tx)
}

async fn finish(tx: Transaction<'_, Postgres>) -> Result<(), AdminError> {
    tx.commit().await.context("finish a search probe")
}
