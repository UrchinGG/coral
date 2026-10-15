use std::collections::HashMap;
use std::net::IpAddr;

use chrono::{DateTime, Utc};
use reqlog::KeyKind;
use serde::Serialize;
use uuid::Uuid;

use super::probe::{Account, DiscordSource, Found, IpTraffic, KeyMatch, Sighting};
use crate::refs::EntityRef;
use crate::serde_id;

pub const EXACT_KEY: u8 = 100;
pub const IDENTIFIED: u8 = 95;
pub const CURRENT_NAME: u8 = 90;
pub const DISCORD_NAME_EXACT: u8 = 85;
pub const IP_WITH_TRAFFIC: u8 = 80;
pub const DISCORD_USER: u8 = 80;
pub const KEY_PREFIX: u8 = 75;
pub const FORMER_NAME: u8 = 70;
pub const BOT_LOOKUPS: u8 = 70;
pub const REQUESTS_NAMING: u8 = 65;
pub const UNPROFILED_PLAYER: u8 = 65;
pub const DISCORD_NAME_PARTIAL: u8 = 60;
pub const GUILD_OR_PLUGIN: u8 = 50;
pub const MEMBER_NUMBER: u8 = 40;
pub const IP_WITHOUT_TRAFFIC: u8 = 30;

pub const LOOKUPS_TAB: &str = "lookups";

const REASON_TIME_FORMAT: &str = "%Y-%m-%d %H:%M UTC";
const REQUEST_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S UTC";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHit {
    #[serde(rename = "ref")]
    pub entity: EntityRef,
    pub score: u8,
    pub exact: bool,
    pub reason: String,
    #[serde(serialize_with = "serde_id::discord_id_opt")]
    pub account_id: Option<i64>,
    pub tab: Option<&'static str>,
}

pub fn rank(found: Vec<Found>, limit: u8) -> Vec<SearchHit> {
    let merged = merged_accounts(&found);
    let requested = requested_players(&found);
    let mut best: HashMap<String, SearchHit> = HashMap::new();
    for hit in found
        .iter()
        .chain(&merged)
        .map(|found| hit_for(found, &requested))
    {
        let identity = hit_identity(&hit);
        match best.get(&identity) {
            Some(kept) if kept.score >= hit.score => {}
            _ => {
                best.insert(identity, hit);
            }
        }
    }
    let mut hits: Vec<(String, SearchHit)> = best.into_iter().collect();
    hits.sort_by(|(a_id, a), (b_id, b)| {
        b.score
            .cmp(&a.score)
            .then(b.exact.cmp(&a.exact))
            .then(a_id.cmp(b_id))
    });
    hits.into_iter()
        .take(usize::from(limit))
        .map(|(_, hit)| hit)
        .collect()
}

pub fn mojang_hit(uuid: Uuid, owners: &[Account], username: &str) -> SearchHit {
    player_hit(
        uuid,
        owners,
        CURRENT_NAME,
        format!("Mojang: {username} is this player's current name"),
    )
}

fn player_hit(uuid: Uuid, owners: &[Account], score: u8, reason: String) -> SearchHit {
    SearchHit {
        entity: EntityRef::Player { uuid },
        score,
        exact: true,
        reason,
        account_id: sole_owner(owners),
        tab: None,
    }
}

fn merged_accounts(found: &[Found]) -> Vec<Found> {
    found
        .iter()
        .filter_map(|candidate| match candidate {
            Found::DiscordName {
                account,
                username,
                exact: true,
            } if owns_a_found_player(found, account.discord_id) => Some(Found::Merged {
                account: *account,
                username: username.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn owns_a_found_player(found: &[Found], discord_id: i64) -> bool {
    found.iter().any(|candidate| match candidate {
        Found::Player { owners, .. } | Found::NameHolder { owners, .. } => {
            sole_owner(owners) == Some(discord_id)
        }
        _ => false,
    })
}

fn requested_players(found: &[Found]) -> HashMap<Uuid, DateTime<Utc>> {
    found
        .iter()
        .filter_map(|found| match found {
            Found::PlayerRequests { uuid, last } => Some((*uuid, *last)),
            _ => None,
        })
        .collect()
}

fn hit_for(found: &Found, requested: &HashMap<Uuid, DateTime<Utc>>) -> SearchHit {
    match found {
        Found::Member { account, by_number } => member_hit(*account, *by_number),
        Found::DiscordUser { discord_id, source } => discord_user_hit(*discord_id, *source),
        Found::BotLookups { account, last } => bot_lookups_hit(*account, *last),
        Found::Player {
            uuid,
            dashed,
            owners,
        } => known_player_hit(*uuid, *dashed, owners, requested.get(uuid)),
        Found::PlayerRequests { uuid, last } => unprofiled_player_hit(*uuid, *last),
        Found::Request {
            request_id,
            ts,
            method,
            target,
            status,
        } => request_hit(*request_id, *ts, method, target, *status),
        Found::Key { key, exact } => key_hit(key, *exact),
        Found::UnknownPrefix {
            prefix,
            kind,
            last_hour,
        } => unknown_prefix_hit(prefix, *kind, *last_hour),
        Found::Ip { ip, traffic } => ip_hit(*ip, traffic.as_ref()),
        Found::NameHolder {
            uuid,
            current,
            sighting,
            owners,
            ..
        } => name_holder_hit(*uuid, *current, *sighting, owners),
        Found::NamedRequests { name, last } => named_requests_hit(name, *last),
        Found::DiscordName {
            account,
            username,
            exact,
        } => discord_name_hit(*account, username, *exact),
        Found::Plugin { slug, name, exact } => plugin_hit(slug, name, *exact),
        Found::Guild {
            guild_id,
            name,
            exact,
        } => guild_hit(guild_id, name, *exact),
        Found::Merged { account, username } => merged_hit(*account, username),
    }
}

fn member_hit(account: Account, by_number: bool) -> SearchHit {
    let member = member_label(account);
    let (score, reason) = match by_number {
        true => (MEMBER_NUMBER, format!("Coral {member}")),
        false => (IDENTIFIED, format!("Discord id of {member}")),
    };
    account_hit(account, score, true, reason)
}

fn discord_user_hit(discord_id: i64, source: DiscordSource) -> SearchHit {
    let reason = match source {
        DiscordSource::Starfish => "Discord id of a Starfish user without a Coral membership",
        DiscordSource::NameCache => "Discord id of a known Discord user without a Coral membership",
    };
    account_hit(
        Account {
            discord_id,
            member_id: None,
        },
        DISCORD_USER,
        true,
        reason.to_string(),
    )
}

fn bot_lookups_hit(account: Account, last: DateTime<Utc>) -> SearchHit {
    SearchHit {
        tab: Some(LOOKUPS_TAB),
        ..account_hit(
            account,
            BOT_LOOKUPS,
            true,
            format!("Ran bot lookups, the newest at {}", reason_time(last)),
        )
    }
}

fn known_player_hit(
    uuid: Uuid,
    dashed: bool,
    owners: &[Account],
    requested: Option<&DateTime<Utc>>,
) -> SearchHit {
    let form = match dashed {
        true => "Dashed UUID",
        false => "Undashed UUID",
    };
    let mut reason = format!("{form} of a known player{}", ownership(owners));
    if let Some(last) = requested {
        reason.push_str(&format!(
            "; last named in a request at {}",
            reason_time(*last)
        ));
    }
    player_hit(uuid, owners, IDENTIFIED, reason)
}

fn unprofiled_player_hit(uuid: Uuid, last: DateTime<Utc>) -> SearchHit {
    player_hit(
        uuid,
        &[],
        UNPROFILED_PLAYER,
        format!(
            "Requests for {} (no profile), the newest at {}",
            uuid.simple(),
            reason_time(last)
        ),
    )
}

fn request_hit(
    request_id: Uuid,
    ts: DateTime<Utc>,
    method: &str,
    target: &str,
    status: i16,
) -> SearchHit {
    SearchHit {
        entity: EntityRef::Request { request_id, ts },
        score: IDENTIFIED,
        exact: true,
        reason: format!(
            "{method} {target} · {status} · {}",
            ts.format(REQUEST_TIME_FORMAT)
        ),
        account_id: None,
        tab: None,
    }
}

fn key_hit(key: &KeyMatch, exact: bool) -> SearchHit {
    let kind = key.kind.label();
    if exact && !key.retired {
        return account_hit(
            key.owner,
            EXACT_KEY,
            true,
            format!(
                "Exact match on the current {kind} key of {}",
                member_label(key.owner)
            ),
        );
    }
    let state = match key.retired {
        true => "retired",
        false => "current",
    };
    let (score, reason) = match exact {
        true => (EXACT_KEY, format!("Exact match on a retired {kind} key")),
        false => (KEY_PREFIX, format!("Prefix of a {state} {kind} key")),
    };
    SearchHit {
        entity: EntityRef::Key {
            key_id: key.key_id,
            prefix: key.prefix.clone(),
            key_kind: key.kind,
        },
        score,
        exact,
        reason,
        account_id: Some(key.owner.discord_id),
        tab: None,
    }
}

fn unknown_prefix_hit(prefix: &str, kind: KeyKind, last_hour: DateTime<Utc>) -> SearchHit {
    SearchHit {
        entity: EntityRef::Key {
            key_id: None,
            prefix: prefix.to_string(),
            key_kind: kind,
        },
        score: KEY_PREFIX,
        exact: false,
        reason: format!(
            "Matches no stored key; requests presented it as recently as the hour from {}",
            reason_time(last_hour)
        ),
        account_id: None,
        tab: None,
    }
}

fn ip_hit(ip: IpAddr, traffic: Option<&IpTraffic>) -> SearchHit {
    let (score, reason, country, proxied) = match traffic {
        Some(traffic) => (
            IP_WITH_TRAFFIC,
            format!(
                "{} requests from {} account{}, last seen {}",
                traffic.requests,
                traffic.accounts,
                plural(traffic.accounts),
                reason_time(traffic.last_seen)
            ),
            traffic.country.clone(),
            traffic.proxied,
        ),
        None => (
            IP_WITHOUT_TRAFFIC,
            "No account traffic recorded from this address".to_string(),
            None,
            false,
        ),
    };
    SearchHit {
        entity: EntityRef::Ip {
            ip: ip.to_string(),
            country,
            proxied,
        },
        score,
        exact: true,
        reason,
        account_id: None,
        tab: None,
    }
}

fn name_holder_hit(
    uuid: Uuid,
    current: bool,
    sighting: Option<Sighting>,
    owners: &[Account],
) -> SearchHit {
    let owned = ownership(owners);
    let (score, reason) = match (current, sighting) {
        (true, _) => (CURRENT_NAME, format!("Current owner of the name{owned}")),
        (false, Some(Sighting::Last(at))) => (
            FORMER_NAME,
            format!(
                "Former owner of the name, last seen with it {}{owned}",
                reason_time(at)
            ),
        ),
        (false, Some(Sighting::First(at))) => (
            FORMER_NAME,
            format!(
                "Former owner of the name, first seen with it {}{owned}",
                reason_time(at)
            ),
        ),
        (false, None) => (FORMER_NAME, format!("Former owner of the name{owned}")),
    };
    player_hit(uuid, owners, score, reason)
}

fn named_requests_hit(name: &str, last: DateTime<Utc>) -> SearchHit {
    SearchHit {
        entity: EntityRef::Name {
            name: name.to_string(),
        },
        score: REQUESTS_NAMING,
        exact: true,
        reason: format!(
            "Requests naming \"{name}\" by name, the newest at {}",
            reason_time(last)
        ),
        account_id: None,
        tab: None,
    }
}

fn discord_name_hit(account: Account, username: &str, exact: bool) -> SearchHit {
    let (score, reason) = match exact {
        true => (DISCORD_NAME_EXACT, format!("Discord username @{username}")),
        false => (
            DISCORD_NAME_PARTIAL,
            format!("Discord username @{username} partly matches"),
        ),
    };
    account_hit(account, score, exact, reason)
}

fn plugin_hit(slug: &str, name: &str, exact: bool) -> SearchHit {
    SearchHit {
        entity: EntityRef::Plugin {
            slug: slug.to_string(),
            name: Some(name.to_string()),
        },
        score: GUILD_OR_PLUGIN,
        exact,
        reason: format!("Plugin {name} ({slug})"),
        account_id: None,
        tab: None,
    }
}

fn guild_hit(guild_id: &str, name: &str, exact: bool) -> SearchHit {
    SearchHit {
        entity: EntityRef::Guild {
            guild_id: guild_id.to_string(),
            name: Some(name.to_string()),
        },
        score: GUILD_OR_PLUGIN,
        exact,
        reason: format!("Guild {name}"),
        account_id: None,
        tab: None,
    }
}

fn merged_hit(account: Account, username: &str) -> SearchHit {
    account_hit(
        account,
        IDENTIFIED,
        true,
        format!("Discord username @{username}; the same account owns a player with this name"),
    )
}

fn account_hit(account: Account, score: u8, exact: bool, reason: String) -> SearchHit {
    SearchHit {
        entity: EntityRef::Account {
            discord_id: account.discord_id,
            member_id: account.member_id,
        },
        score,
        exact,
        reason,
        account_id: Some(account.discord_id),
        tab: None,
    }
}

fn member_label(account: Account) -> String {
    match account.member_id {
        Some(member_id) => format!("member #{member_id}"),
        None => "a Discord user without a Coral membership".to_string(),
    }
}

fn ownership(owners: &[Account]) -> String {
    match owners {
        [] => String::new(),
        [_] => "; linked to an account".to_string(),
        many => format!("; linked to {} accounts", many.len()),
    }
}

fn sole_owner(owners: &[Account]) -> Option<i64> {
    match owners {
        [owner] => Some(owner.discord_id),
        _ => None,
    }
}

fn hit_identity(hit: &SearchHit) -> String {
    let id = match &hit.entity {
        EntityRef::Account { discord_id, .. } => discord_id.to_string(),
        EntityRef::Player { uuid } => uuid.simple().to_string(),
        EntityRef::Name { name } => name.clone(),
        EntityRef::Request { request_id, .. } => request_id.simple().to_string(),
        EntityRef::Ip { ip, .. } => ip.clone(),
        EntityRef::Key { key_id, prefix, .. } => {
            key_id.map_or_else(|| prefix.clone(), |id| id.to_string())
        }
        EntityRef::Guild { guild_id, .. } => guild_id.clone(),
        EntityRef::Plugin { slug, .. } => slug.clone(),
        EntityRef::Internal { service } => service.code().to_string(),
        EntityRef::Anonymous => String::new(),
    };
    format!(
        "{}:{id}:{}",
        hit_kind(&hit.entity),
        hit.tab.unwrap_or_default()
    )
}

fn hit_kind(entity: &EntityRef) -> &'static str {
    match entity {
        EntityRef::Account { .. } => "account",
        EntityRef::Player { .. } => "player",
        EntityRef::Name { .. } => "name",
        EntityRef::Request { .. } => "request",
        EntityRef::Ip { .. } => "ip",
        EntityRef::Key { .. } => "key",
        EntityRef::Guild { .. } => "guild",
        EntityRef::Plugin { .. } => "plugin",
        EntityRef::Internal { .. } => "internal",
        EntityRef::Anonymous => "anonymous",
    }
}

fn reason_time(at: DateTime<Utc>) -> String {
    at.format(REASON_TIME_FORMAT).to_string()
}

fn plural(count: i64) -> &'static str {
    match count {
        1 => "",
        _ => "s",
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::search::DEFAULT_HIT_LIMIT;

    const OWNER: Account = Account {
        discord_id: 910_000_000_000_000_001,
        member_id: Some(910_001),
    };
    const OTHER: Account = Account {
        discord_id: 910_000_000_000_000_002,
        member_id: Some(910_002),
    };
    const SOLE: &str = "a11ce000000040008000000000000a01";
    const SHARED: &str = "a11ce000000040008000000000000a02";

    fn holder(hex: &str, owners: Vec<Account>) -> Found {
        Found::NameHolder {
            uuid: Uuid::parse_str(hex).unwrap(),
            name: "kestrel_".into(),
            current: true,
            sighting: None,
            owners,
        }
    }

    fn player(hex: &str) -> EntityRef {
        EntityRef::Player {
            uuid: Uuid::parse_str(hex).unwrap(),
        }
    }

    #[test]
    fn a_discord_name_merges_only_with_a_player_its_account_owns_alone() {
        let found = vec![
            Found::DiscordName {
                account: OWNER,
                username: "kestrel_".into(),
                exact: true,
            },
            holder(SOLE, vec![OWNER]),
            holder(SHARED, vec![OWNER, OTHER]),
        ];
        let hits = rank(found, DEFAULT_HIT_LIMIT);
        let entities: Vec<&EntityRef> = hits.iter().map(|hit| &hit.entity).collect();
        assert_eq!(
            entities,
            vec![
                &EntityRef::Account {
                    discord_id: OWNER.discord_id,
                    member_id: OWNER.member_id,
                },
                &player(SOLE),
                &player(SHARED),
            ]
        );
        assert_eq!(hits[0].score, IDENTIFIED);
        assert_eq!(hits[1].account_id, Some(OWNER.discord_id));
        assert_eq!(hits[2].account_id, None);
    }

    #[test]
    fn a_discord_name_without_an_owned_player_stays_a_discord_name_hit() {
        let found = vec![
            Found::DiscordName {
                account: OWNER,
                username: "kestrel_".into(),
                exact: true,
            },
            holder(SHARED, vec![OWNER, OTHER]),
        ];
        let hits = rank(found, DEFAULT_HIT_LIMIT);
        let scores: Vec<(&EntityRef, u8)> =
            hits.iter().map(|hit| (&hit.entity, hit.score)).collect();
        assert_eq!(
            scores,
            vec![
                (&player(SHARED), CURRENT_NAME),
                (
                    &EntityRef::Account {
                        discord_id: OWNER.discord_id,
                        member_id: OWNER.member_id,
                    },
                    DISCORD_NAME_EXACT
                ),
            ]
        );
    }

    #[test]
    fn each_entity_keeps_its_best_hit_and_the_limit_cuts_the_tail() {
        let uuid = Uuid::parse_str(SOLE).unwrap();
        let last = Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap();
        let found = vec![
            Found::PlayerRequests { uuid, last },
            Found::Player {
                uuid,
                dashed: false,
                owners: vec![OWNER],
            },
            Found::Plugin {
                slug: "kestrel-tools".into(),
                name: "Kestrel tools".into(),
                exact: false,
            },
        ];
        let hits = rank(found, 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entity, player(SOLE));
        assert_eq!(hits[0].score, IDENTIFIED);
        assert!(
            hits[0]
                .reason
                .contains("last named in a request at 2026-10-14 12:00 UTC")
        );
    }
}
