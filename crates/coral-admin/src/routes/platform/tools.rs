use std::collections::HashMap;
use std::net::IpAddr;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use futures_util::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::page::{Accounts, Mentions};
use crate::error::{AdminError, Context};
use crate::query::PlayerInput;
use crate::refs::{EntityRef, Labels};
use crate::state::AppState;

pub const MAX_RESOLVE_INPUTS: usize = 200;

const NAME_LOOKUP_CONCURRENCY: usize = 6;
const SNOWFLAKE_DIGITS: std::ops::RangeInclusive<usize> = 17..=20;
const MENTION_PREFIXES: [&str; 2] = ["<@!", "<@"];
const MENTION_SUFFIX: &str = ">";
const QUOTES: [char; 3] = ['"', '\'', '`'];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveBody {
    inputs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Resolved {
    results: Vec<Resolution>,
    labels: Labels,
}

#[derive(Debug, Serialize)]
struct Resolution {
    input: String,
    kind: InputKind,
    entity: Option<EntityRef>,
    note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum InputKind {
    Discord,
    Player,
    Name,
    Ip,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Parsed {
    Discord(i64),
    Player(Uuid),
    Name(String),
    Ip(IpAddr),
    Unknown,
}

pub async fn resolve(
    State(state): State<AppState>,
    body: Result<Json<ResolveBody>, JsonRejection>,
) -> Result<Json<Resolved>, AdminError> {
    let Json(body) = body?;
    let inputs: Vec<String> = body
        .inputs
        .iter()
        .map(|input| input.trim().to_string())
        .filter(|input| !input.is_empty())
        .collect();
    if inputs.len() > MAX_RESOLVE_INPUTS {
        return Err(AdminError::BadRequest(format!(
            "resolve at most {MAX_RESOLVE_INPUTS} identifiers at once"
        )));
    }
    let parsed: Vec<Parsed> = inputs.iter().map(|input| parse(input)).collect();
    let discord = discord_ids(&parsed);
    let (players, accounts, ips) = tokio::try_join!(
        name_owners(&state, &parsed),
        Accounts::load(&state, &discord),
        known_ips(&state, &parsed),
    )?;
    let results: Vec<Resolution> = inputs
        .into_iter()
        .zip(parsed)
        .map(|(input, parsed)| resolution(input, parsed, &players, &accounts, &ips))
        .collect();
    let labels = mentions_of(&results).labels(&state).await?;
    Ok(Json(Resolved { results, labels }))
}

fn parse(input: &str) -> Parsed {
    let text = unwrap_mention(input.trim_matches(QUOTES));
    if SNOWFLAKE_DIGITS.contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse().map_or(Parsed::Unknown, Parsed::Discord);
    }
    if let Ok(ip) = text.parse::<IpAddr>() {
        return Parsed::Ip(ip);
    }
    match PlayerInput::parse(text) {
        Ok(PlayerInput::Uuid(uuid)) => Parsed::Player(uuid),
        Ok(PlayerInput::Name(name)) => Parsed::Name(name),
        Err(_) => Parsed::Unknown,
    }
}

fn unwrap_mention(text: &str) -> &str {
    MENTION_PREFIXES
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix)?.strip_suffix(MENTION_SUFFIX))
        .unwrap_or(text)
}

async fn name_owners(
    state: &AppState,
    parsed: &[Parsed],
) -> Result<HashMap<String, Vec<Uuid>>, AdminError> {
    let mut names: Vec<String> = parsed
        .iter()
        .filter_map(|parsed| match parsed {
            Parsed::Name(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    stream::iter(names)
        .map(|name| async move {
            let keys = state.players.keys(&PlayerInput::Name(name.clone())).await?;
            Ok::<_, AdminError>((name, keys.uuids.clone()))
        })
        .buffer_unordered(NAME_LOOKUP_CONCURRENCY)
        .try_collect()
        .await
}

async fn known_ips(
    state: &AppState,
    parsed: &[Parsed],
) -> Result<HashMap<IpAddr, i64>, AdminError> {
    let ips: Vec<String> = parsed
        .iter()
        .filter_map(|parsed| match parsed {
            Parsed::Ip(ip) => Some(ip.to_string()),
            _ => None,
        })
        .collect();
    if ips.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT host(ip_address), count(DISTINCT member_id) FROM api_key_ips
         WHERE ip_address = ANY($1::inet[])
         GROUP BY ip_address",
    )
    .bind(&ips)
    .fetch_all(state.db.pool())
    .await
    .context("count the accounts seen on the IPs")?;
    Ok(rows
        .into_iter()
        .filter_map(|(ip, accounts)| Some((ip.parse().ok()?, accounts)))
        .collect())
}

fn discord_ids(parsed: &[Parsed]) -> Vec<i64> {
    parsed
        .iter()
        .filter_map(|parsed| match parsed {
            Parsed::Discord(id) => Some(*id),
            _ => None,
        })
        .collect()
}

fn resolution(
    input: String,
    parsed: Parsed,
    players: &HashMap<String, Vec<Uuid>>,
    accounts: &Accounts,
    ips: &HashMap<IpAddr, i64>,
) -> Resolution {
    let (kind, entity, note) = match parsed {
        Parsed::Discord(id) => discord_resolution(accounts.of(id)),
        Parsed::Player(uuid) => (
            InputKind::Player,
            Some(EntityRef::Player { uuid }),
            "Player UUID".to_string(),
        ),
        Parsed::Name(name) => name_resolution(
            name.clone(),
            players.get(&name).map_or(&[][..], Vec::as_slice),
        ),
        Parsed::Ip(ip) => ip_resolution(ip, ips.get(&ip).copied().unwrap_or(0)),
        Parsed::Unknown => (
            InputKind::Unknown,
            None,
            "Not a Discord id, player UUID, player name or IP".to_string(),
        ),
    };
    Resolution {
        input,
        kind,
        entity,
        note,
    }
}

fn discord_resolution(account: EntityRef) -> (InputKind, Option<EntityRef>, String) {
    let note = match &account {
        EntityRef::Account {
            member_id: Some(member_id),
            ..
        } => format!("Coral member #{member_id}"),
        _ => "Discord user with no Coral account".to_string(),
    };
    (InputKind::Discord, Some(account), note)
}

fn name_resolution(name: String, owners: &[Uuid]) -> (InputKind, Option<EntityRef>, String) {
    match owners {
        [] => (
            InputKind::Name,
            Some(EntityRef::Name { name }),
            "No UUID known for this name; the link opens the requests naming it".to_string(),
        ),
        [only] => (
            InputKind::Name,
            Some(EntityRef::Player { uuid: *only }),
            "Current holder of the name".to_string(),
        ),
        [first, rest @ ..] => (
            InputKind::Name,
            Some(EntityRef::Player { uuid: *first }),
            former_holders(rest.len()),
        ),
    }
}

fn former_holders(count: usize) -> String {
    match count {
        1 => "Current holder; 1 other UUID has held this name".to_string(),
        many => format!("Current holder; {many} other UUIDs have held this name"),
    }
}

fn ip_resolution(ip: IpAddr, accounts: i64) -> (InputKind, Option<EntityRef>, String) {
    let note = match accounts {
        0 => "No account recorded on this IP".to_string(),
        1 => "1 account recorded on this IP".to_string(),
        many => format!("{many} accounts recorded on this IP"),
    };
    let entity = EntityRef::Ip {
        ip: ip.to_string(),
        country: None,
        proxied: false,
    };
    (InputKind::Ip, Some(entity), note)
}

fn mentions_of(results: &[Resolution]) -> Mentions {
    let mut mentions = Mentions::default();
    for entity in results.iter().filter_map(|result| result.entity.as_ref()) {
        match entity {
            EntityRef::Account { discord_id, .. } => mentions.account(*discord_id),
            EntityRef::Player { uuid } => mentions.player(*uuid),
            _ => {}
        }
    }
    mentions
}

#[cfg(test)]
mod tests {
    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    #[test]
    fn each_identifier_shape_is_recognised() {
        assert_eq!(
            parse("<@!1218840276405911602>"),
            Parsed::Discord(1_218_840_276_405_911_602)
        );
        assert_eq!(
            parse("1218840276405911602"),
            Parsed::Discord(1_218_840_276_405_911_602)
        );
        assert_eq!(
            parse(OJUGG),
            Parsed::Player(Uuid::parse_str(OJUGG).unwrap())
        );
        assert_eq!(
            parse("94589c87-999b-4a7a-b3ad-d2ae50711e64"),
            Parsed::Player(Uuid::parse_str(OJUGG).unwrap())
        );
        assert_eq!(parse("\"oJugg\""), Parsed::Name("ojugg".into()));
        assert_eq!(parse("10.4.0.1"), Parsed::Ip("10.4.0.1".parse().unwrap()));
        assert_eq!(
            parse("2001:db8::1"),
            Parsed::Ip("2001:db8::1".parse().unwrap())
        );
        assert_eq!(parse("not a thing!"), Parsed::Unknown);
    }

    #[test]
    fn a_name_resolves_to_its_current_holder_and_counts_former_ones() {
        let first = Uuid::parse_str(OJUGG).unwrap();
        let second = Uuid::from_u128(7);
        let (_, entity, note) = name_resolution("ojugg".into(), &[first, second]);
        assert_eq!(entity, Some(EntityRef::Player { uuid: first }));
        assert_eq!(note, "Current holder; 1 other UUID has held this name");
        assert_eq!(
            former_holders(3),
            "Current holder; 3 other UUIDs have held this name"
        );
        let (_, entity, _) = name_resolution("ghost".into(), &[]);
        assert_eq!(
            entity,
            Some(EntityRef::Name {
                name: "ghost".into()
            })
        );
    }

    #[test]
    fn discord_ids_without_a_member_say_so() {
        let (_, _, note) = discord_resolution(EntityRef::Account {
            discord_id: 1,
            member_id: None,
        });
        assert_eq!(note, "Discord user with no Coral account");
        let (_, _, note) = discord_resolution(EntityRef::Account {
            discord_id: 1,
            member_id: Some(21_111),
        });
        assert_eq!(note, "Coral member #21111");
    }
}
