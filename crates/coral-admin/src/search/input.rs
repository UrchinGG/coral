use std::net::IpAddr;
use std::ops::RangeInclusive;

use url::Url;
use uuid::Uuid;

use super::lookup::{Lookup, Reach};

pub const MAX_QUERY_CHARS: usize = 2_048;
pub const MAX_URL_IDENTIFIERS: usize = 4;
pub const MOJANG_NAME_CHARS: RangeInclusive<usize> = 3..=16;

const DISCORD_ID_DIGITS: RangeInclusive<usize> = 17..=20;
const MEMBER_NUMBER_DIGITS: RangeInclusive<usize> = 1..=9;
const NAME_CHARS: RangeInclusive<usize> = 3..=25;
const SHORT_NAME_CHARS: RangeInclusive<usize> = 1..=2;
const MIN_FUZZY_CHARS: usize = 3;
const GUILD_ID_HEX: usize = 24;
const UNDASHED_UUID_HEX: usize = 32;
const DASHED_UUID_CHARS: usize = 36;
const DASH_POSITIONS: [usize; 4] = [8, 13, 18, 23];
const QUOTES: [char; 3] = ['"', '\'', '`'];
const MENTION_OPEN: &str = "<@";
const NICKNAME_MARK: char = '!';
const MENTION_CLOSE: char = '>';
const OPERATOR_MARK: char = ':';
const URL_SCHEMES: [&str; 2] = ["http://", "https://"];
const IDENTIFIER_PARAMS: [&str; 5] = ["player", "uuid", "name", "uuids", "names"];
const KEY_PARAM: &str = "key";
const LIST_SEPARATOR: char = ',';
const RESOLVE_SEGMENT: &str = "resolve";
const OPERATORS: [(&str, Operator); 8] = [
    ("p", Operator::Player),
    ("a", Operator::Account),
    ("k", Operator::Key),
    ("ip", Operator::Ip),
    ("r", Operator::Request),
    ("g", Operator::Guild),
    ("pl", Operator::Plugin),
    ("n", Operator::Name),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Player,
    Account,
    Key,
    Ip,
    Request,
    Guild,
    Plugin,
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Account,
    Player,
    Name,
    Request,
    Ip,
    Key,
    Guild,
    Plugin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    DiscordId(i64),
    MemberNumber(i64),
    Uuid { uuid: Uuid, dashed: bool },
    GuildId(String),
    KeyPrefix(String),
    Ip(IpAddr),
    Name(String),
    ShortName(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlIdentifier {
    Player(Uuid),
    Name(String),
    Key(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    Shapes(Vec<Shape>),
    Url {
        identifiers: Vec<UrlIdentifier>,
        players_found: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub text: String,
    pub operator: Option<Operator>,
    pub reading: Reading,
}

impl Operator {
    pub fn label(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Account => "account",
            Self::Key => "key",
            Self::Ip => "IP",
            Self::Request => "request",
            Self::Guild => "guild",
            Self::Plugin => "plugin",
            Self::Name => "raw name",
        }
    }

    pub fn admits(self, kind: Kind) -> bool {
        match self {
            Self::Player => kind == Kind::Player,
            Self::Account => kind == Kind::Account,
            Self::Key => kind == Kind::Key,
            Self::Ip => kind == Kind::Ip,
            Self::Request => kind == Kind::Request,
            Self::Guild => kind == Kind::Guild,
            Self::Plugin => kind == Kind::Plugin,
            Self::Name => kind == Kind::Name,
        }
    }
}

impl Shape {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DiscordId(_) => "Discord id",
            Self::MemberNumber(_) => "member number",
            Self::Uuid { uuid, .. } if is_time_ordered(uuid) => "request id (UUIDv7)",
            Self::Uuid { dashed: true, .. } => "dashed UUID",
            Self::Uuid { dashed: false, .. } => "undashed UUID",
            Self::GuildId(_) => "guild id",
            Self::KeyPrefix(_) => "key prefix",
            Self::Ip(_) => "IP address",
            Self::Name(_) => "name",
            Self::ShortName(_) => "short name",
            Self::Text(_) => "text",
        }
    }

    pub fn lookups(&self, operator: Option<Operator>) -> Vec<Lookup> {
        match self {
            Self::DiscordId(id) => vec![Lookup::Account(*id), Lookup::BotLookups(*id)],
            Self::MemberNumber(number) => vec![Lookup::MemberNumber(*number)],
            Self::Uuid { uuid, dashed } => uuid_lookups(*uuid, *dashed, operator),
            Self::GuildId(id) => vec![Lookup::Guild(id.clone())],
            Self::KeyPrefix(prefix) => vec![Lookup::KeyPrefix(prefix.clone())],
            Self::Ip(ip) => vec![Lookup::Ip(*ip)],
            Self::Name(name) => vec![
                Lookup::NameHolders {
                    name: name.clone(),
                    history: true,
                },
                Lookup::NamedRequests(name.clone()),
                Lookup::DiscordName(name.clone(), Reach::Prefix),
                Lookup::Plugin(name.clone(), Reach::Prefix),
                Lookup::GuildName(name.clone(), Reach::Prefix),
            ],
            Self::ShortName(name) => vec![
                Lookup::NameHolders {
                    name: name.clone(),
                    history: false,
                },
                Lookup::DiscordName(name.clone(), Reach::Exact),
            ],
            Self::Text(text) if text.chars().count() < MIN_FUZZY_CHARS => Vec::new(),
            Self::Text(text) => vec![
                Lookup::DiscordName(text.clone(), Reach::Contains),
                Lookup::Plugin(text.clone(), Reach::Contains),
                Lookup::GuildName(text.clone(), Reach::Contains),
            ],
        }
    }
}

impl UrlIdentifier {
    fn lookups(&self) -> Vec<Lookup> {
        match self {
            Self::Player(uuid) => vec![
                Lookup::Player {
                    uuid: *uuid,
                    dashed: false,
                },
                Lookup::PlayerRequests(*uuid),
            ],
            Self::Name(name) => vec![
                Lookup::NameHolders {
                    name: name.clone(),
                    history: true,
                },
                Lookup::NamedRequests(name.clone()),
            ],
            Self::Key(key) => vec![Lookup::ApiKey(*key)],
        }
    }
}

impl Input {
    pub fn parse(raw: &str) -> Self {
        let normalized = normalize(raw);
        let (operator, text) = split_operator(&normalized);
        let reading = match url_identifiers(&text) {
            Some((identifiers, players_found)) => Reading::Url {
                identifiers,
                players_found,
            },
            None => Reading::Shapes(match operator {
                Some(_) => every_shape(&text),
                None => leading_shapes(&text),
            }),
        };
        Self {
            text,
            operator,
            reading,
        }
    }

    pub fn lookups(&self) -> Vec<Lookup> {
        let candidates: Vec<Lookup> = match &self.reading {
            Reading::Shapes(shapes) => shapes
                .iter()
                .flat_map(|shape| shape.lookups(self.operator))
                .collect(),
            Reading::Url { identifiers, .. } => identifiers
                .iter()
                .flat_map(UrlIdentifier::lookups)
                .collect(),
        };
        let mut lookups: Vec<Lookup> = Vec::new();
        let forced_name = self
            .raw_name()
            .map(|name| Lookup::NamedRequests(name.to_ascii_lowercase()));
        for lookup in candidates.into_iter().chain(forced_name) {
            if self.admits_any(lookup.kinds()) && !lookups.contains(&lookup) {
                lookups.push(lookup);
            }
        }
        lookups
    }

    pub fn raw_name(&self) -> Option<&str> {
        let forced = self.operator == Some(Operator::Name);
        (forced && reqlog::is_player_name(&self.text)).then_some(self.text.as_str())
    }

    pub fn remote_name(&self) -> Option<&str> {
        let Reading::Shapes(shapes) = &self.reading else {
            return None;
        };
        let named = shapes.iter().any(|shape| matches!(shape, Shape::Name(_)));
        (named && self.admits(Kind::Player) && is_mojang_name(&self.text))
            .then_some(self.text.as_str())
    }

    pub fn admits(&self, kind: Kind) -> bool {
        self.operator.is_none_or(|operator| operator.admits(kind))
    }

    pub fn interpretations(&self) -> Vec<String> {
        let forced = self
            .operator
            .map(|operator| format!("forced: {}", operator.label()));
        let readings = match &self.reading {
            Reading::Url {
                identifiers,
                players_found,
            } => vec![url_reading(identifiers, *players_found)],
            Reading::Shapes(shapes) => shapes
                .iter()
                .filter(|shape| self.applies(shape))
                .map(|shape| shape.label().to_string())
                .collect(),
        };
        forced.into_iter().chain(readings).collect()
    }

    fn applies(&self, shape: &Shape) -> bool {
        let named = matches!(shape, Shape::Name(_) | Shape::ShortName(_));
        let probed = shape
            .lookups(self.operator)
            .iter()
            .any(|lookup| self.admits_any(lookup.kinds()));
        probed || (named && self.raw_name().is_some())
    }

    fn admits_any(&self, kinds: &[Kind]) -> bool {
        kinds.iter().any(|kind| self.admits(*kind))
    }
}

pub fn is_mojang_name(text: &str) -> bool {
    MOJANG_NAME_CHARS.contains(&text.len()) && reqlog::is_player_name(text)
}

pub fn is_time_ordered(uuid: &Uuid) -> bool {
    uuid.get_version_num() == reqlog::TIME_ORDERED_VERSION
}

fn uuid_lookups(uuid: Uuid, dashed: bool, operator: Option<Operator>) -> Vec<Lookup> {
    if is_time_ordered(&uuid) {
        return vec![Lookup::Request(uuid)];
    }
    let key_wanted = dashed || operator == Some(Operator::Key);
    let mut lookups = vec![
        Lookup::Player { uuid, dashed },
        Lookup::PlayerRequests(uuid),
        Lookup::Request(uuid),
    ];
    if key_wanted {
        lookups.push(Lookup::ApiKey(uuid));
    }
    lookups
}

fn normalize(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(id) = mention(trimmed) {
        return id.to_string();
    }
    trimmed.trim_matches(QUOTES).trim().to_string()
}

fn mention(text: &str) -> Option<&str> {
    let inner = text
        .strip_prefix(MENTION_OPEN)?
        .strip_suffix(MENTION_CLOSE)?;
    let id = inner.strip_prefix(NICKNAME_MARK).unwrap_or(inner);
    (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then_some(id)
}

fn split_operator(text: &str) -> (Option<Operator>, String) {
    let Some((head, rest)) = text.split_once(OPERATOR_MARK) else {
        return (None, text.to_string());
    };
    let operator = OPERATORS
        .iter()
        .find(|(prefix, _)| prefix.eq_ignore_ascii_case(head))
        .map(|(_, operator)| *operator);
    match operator {
        Some(operator) => (Some(operator), normalize(rest)),
        None => (None, text.to_string()),
    }
}

fn leading_shapes(text: &str) -> Vec<Shape> {
    if text.is_empty() {
        return Vec::new();
    }
    if let Some(id) = discord_id(text) {
        return vec![Shape::DiscordId(id)];
    }
    if let Some(shape) = uuid_shape(text) {
        return vec![shape];
    }
    if is_hex(text, GUILD_ID_HEX) {
        return vec![Shape::GuildId(text.to_ascii_lowercase())];
    }
    if let Ok(ip) = text.parse::<IpAddr>() {
        return vec![Shape::Ip(ip)];
    }
    let number = member_number(text).map(Shape::MemberNumber);
    let prefix = key_prefix(text).map(Shape::KeyPrefix);
    number
        .into_iter()
        .chain(prefix)
        .chain([name_shape(text)])
        .collect()
}

fn every_shape(text: &str) -> Vec<Shape> {
    if text.is_empty() {
        return Vec::new();
    }
    let specific = [
        discord_id(text).map(Shape::DiscordId),
        member_number(text).map(Shape::MemberNumber),
        uuid_shape(text),
        is_hex(text, GUILD_ID_HEX).then(|| Shape::GuildId(text.to_ascii_lowercase())),
        key_prefix(text).map(Shape::KeyPrefix),
        text.parse::<IpAddr>().ok().map(Shape::Ip),
    ];
    specific
        .into_iter()
        .flatten()
        .chain([name_shape(text)])
        .collect()
}

fn name_shape(text: &str) -> Shape {
    let lower = text.to_ascii_lowercase();
    let name_like = reqlog::is_player_name(text);
    match text.len() {
        length if name_like && NAME_CHARS.contains(&length) => Shape::Name(lower),
        length if name_like && SHORT_NAME_CHARS.contains(&length) => Shape::ShortName(lower),
        _ => Shape::Text(text.to_lowercase()),
    }
}

fn discord_id(text: &str) -> Option<i64> {
    let digits = DISCORD_ID_DIGITS.contains(&text.len()) && is_digits(text);
    digits
        .then(|| text.parse::<i64>().ok())
        .flatten()
        .filter(|id| *id > 0)
}

fn member_number(text: &str) -> Option<i64> {
    let digits = MEMBER_NUMBER_DIGITS.contains(&text.len()) && is_digits(text);
    digits
        .then(|| text.parse::<i64>().ok())
        .flatten()
        .filter(|number| *number > 0)
}

fn key_prefix(text: &str) -> Option<String> {
    is_hex(text, reqlog::KEY_PREFIX_CHARS).then(|| text.to_ascii_lowercase())
}

fn uuid_shape(text: &str) -> Option<Shape> {
    let dashed = match text.len() {
        UNDASHED_UUID_HEX if is_hex(text, UNDASHED_UUID_HEX) => false,
        DASHED_UUID_CHARS if is_dashed_uuid(text) => true,
        _ => return None,
    };
    let uuid = Uuid::try_parse(text).ok()?;
    Some(Shape::Uuid { uuid, dashed })
}

fn is_dashed_uuid(text: &str) -> bool {
    text.char_indices()
        .all(|(index, c)| match DASH_POSITIONS.contains(&index) {
            true => c == '-',
            false => c.is_ascii_hexdigit(),
        })
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

fn url_identifiers(text: &str) -> Option<(Vec<UrlIdentifier>, usize)> {
    let is_web = URL_SCHEMES.iter().any(|scheme| {
        text.get(..scheme.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
    });
    if !is_web {
        return None;
    }
    let url = Url::parse(text).ok()?;
    let mut identifiers: Vec<UrlIdentifier> = Vec::new();
    for value in query_identifiers(&url).chain(resolved_identifier(&url)) {
        let identifier = match reqlog::parse_uuid(&value) {
            Some(uuid) => UrlIdentifier::Player(uuid),
            None if reqlog::is_player_name(&value) => {
                UrlIdentifier::Name(value.to_ascii_lowercase())
            }
            None => continue,
        };
        if !identifiers.contains(&identifier) {
            identifiers.push(identifier);
        }
    }
    let players_found = identifiers.len();
    identifiers.truncate(MAX_URL_IDENTIFIERS);
    identifiers.extend(url_key(&url));
    Some((identifiers, players_found))
}

fn query_identifiers(url: &Url) -> impl Iterator<Item = String> + '_ {
    url.query_pairs()
        .filter(|(name, _)| IDENTIFIER_PARAMS.contains(&name.as_ref()))
        .flat_map(|(_, value)| {
            value
                .split(LIST_SEPARATOR)
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
}

fn resolved_identifier(url: &Url) -> Option<String> {
    let mut segments = url.path_segments()?;
    segments.find(|segment| *segment == RESOLVE_SEGMENT)?;
    let tail = segments.next().filter(|tail| !tail.is_empty())?;
    Some(urlencoding::decode(tail).ok()?.into_owned())
}

fn url_key(url: &Url) -> Option<UrlIdentifier> {
    url.query_pairs()
        .find(|(name, _)| name == KEY_PARAM)
        .and_then(|(_, value)| reqlog::parse_uuid(value.trim()))
        .map(UrlIdentifier::Key)
}

fn url_reading(identifiers: &[UrlIdentifier], players_found: usize) -> String {
    let keys = identifiers
        .iter()
        .filter(|identifier| matches!(identifier, UrlIdentifier::Key(_)))
        .count();
    let players = identifiers.len() - keys;
    let mut reading = format!(
        "API URL with {players} player identifier{}",
        plural(players)
    );
    if keys > 0 {
        reading.push_str(" and a key");
    }
    if players_found > players {
        reading.push_str(&format!(
            " (the first {players} of {players_found} are searched)"
        ));
    }
    reading
}

fn plural(count: usize) -> &'static str {
    match count {
        1 => "",
        _ => "s",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";
    const OJUGG_DASHED: &str = "94589c87-999b-4a7a-b3ad-d2ae50711e64";
    const REQUEST_V7: &str = "019a1f4e-7b4d-7e8f-9a6b-1c3d5e7f9012";

    fn shapes(raw: &str) -> Vec<Shape> {
        match Input::parse(raw).reading {
            Reading::Shapes(shapes) => shapes,
            Reading::Url { .. } => panic!("{raw} read as a URL"),
        }
    }

    fn ojugg() -> Uuid {
        Uuid::parse_str(OJUGG).unwrap()
    }

    #[test]
    fn mentions_quotes_and_whitespace_are_stripped() {
        assert_eq!(
            Input::parse(" <@!354598337325826048> ").text,
            "354598337325826048"
        );
        assert_eq!(
            Input::parse("<@354598337325826048>").text,
            "354598337325826048"
        );
        assert_eq!(Input::parse("  \"oJugg\"  ").text, "oJugg");
        assert_eq!(Input::parse("`kes_tr`").text, "kes_tr");
    }

    #[test]
    fn each_input_reads_as_the_shapes_the_spec_lists() {
        let cases: [(&str, Vec<Shape>); 13] = [
            (
                "354598337325826048",
                vec![Shape::DiscordId(354_598_337_325_826_048)],
            ),
            (
                "23025",
                vec![Shape::MemberNumber(23025), Shape::Name("23025".into())],
            ),
            (
                "42",
                vec![Shape::MemberNumber(42), Shape::ShortName("42".into())],
            ),
            (
                OJUGG,
                vec![Shape::Uuid {
                    uuid: ojugg(),
                    dashed: false,
                }],
            ),
            (
                &OJUGG_DASHED.to_uppercase(),
                vec![Shape::Uuid {
                    uuid: ojugg(),
                    dashed: true,
                }],
            ),
            (
                "5c4a1f2e9b8d7c6a5f4e3d2c",
                vec![Shape::GuildId("5c4a1f2e9b8d7c6a5f4e3d2c".into())],
            ),
            (
                "E91C0D3A",
                vec![
                    Shape::KeyPrefix("e91c0d3a".into()),
                    Shape::Name("e91c0d3a".into()),
                ],
            ),
            (
                "40213977",
                vec![
                    Shape::MemberNumber(40_213_977),
                    Shape::KeyPrefix("40213977".into()),
                    Shape::Name("40213977".into()),
                ],
            ),
            (
                "198.51.100.23",
                vec![Shape::Ip("198.51.100.23".parse().unwrap())],
            ),
            (
                "2001:db8::1",
                vec![Shape::Ip("2001:db8::1".parse().unwrap())],
            ),
            ("Kestrel_", vec![Shape::Name("kestrel_".into())]),
            ("x_", vec![Shape::ShortName("x_".into())]),
            ("kestrel.dev", vec![Shape::Text("kestrel.dev".into())]),
        ];
        for (raw, expected) in cases {
            assert_eq!(shapes(raw), expected, "{raw}");
        }
    }

    #[test]
    fn a_snowflake_out_of_range_reads_as_a_name() {
        assert_eq!(
            shapes("99999999999999999999"),
            vec![Shape::Name("99999999999999999999".into())]
        );
    }

    #[test]
    fn a_dash_in_the_wrong_place_is_not_a_uuid() {
        assert_eq!(
            shapes("94589c87999b-4a7a-b3ad-d2ae-50711e64"),
            vec![Shape::Text("94589c87999b-4a7a-b3ad-d2ae-50711e64".into())]
        );
    }

    #[test]
    fn operators_force_a_type_and_read_every_applicable_shape() {
        let input = Input::parse("P: 354598337325826048");
        assert_eq!(input.operator, Some(Operator::Player));
        assert_eq!(
            input.lookups(),
            vec![Lookup::NameHolders {
                name: "354598337325826048".into(),
                history: true,
            }]
        );
        assert_eq!(input.interpretations(), vec!["forced: player", "name"]);

        let account = Input::parse("a:kestrel");
        assert_eq!(
            account.lookups(),
            vec![Lookup::DiscordName("kestrel".into(), Reach::Prefix)]
        );

        let pl = Input::parse("pl:bed");
        assert_eq!(pl.operator, Some(Operator::Plugin));
        assert_eq!(
            pl.lookups(),
            vec![Lookup::Plugin("bed".into(), Reach::Prefix)]
        );
    }

    #[test]
    fn an_unknown_prefix_is_part_of_the_text() {
        let input = Input::parse("xyz:abc");
        assert_eq!(input.operator, None);
        assert_eq!(input.text, "xyz:abc");
    }

    #[test]
    fn dashed_uuids_probe_keys_and_undashed_ones_only_when_forced() {
        let dashed = Input::parse(OJUGG_DASHED).lookups();
        assert!(dashed.contains(&Lookup::ApiKey(ojugg())));
        assert!(dashed.contains(&Lookup::Request(ojugg())));
        assert!(dashed.contains(&Lookup::Player {
            uuid: ojugg(),
            dashed: true
        }));

        let undashed = Input::parse(OJUGG).lookups();
        assert!(!undashed.contains(&Lookup::ApiKey(ojugg())));

        let forced = Input::parse(&format!("k:{OJUGG}")).lookups();
        assert_eq!(forced, vec![Lookup::ApiKey(ojugg())]);
    }

    #[test]
    fn a_time_ordered_uuid_is_only_a_request_id() {
        let request = Uuid::parse_str(REQUEST_V7).unwrap();
        assert_eq!(
            Input::parse(REQUEST_V7).lookups(),
            vec![Lookup::Request(request)]
        );
        assert_eq!(
            Input::parse(REQUEST_V7).interpretations(),
            vec!["request id (UUIDv7)"]
        );
    }

    #[test]
    fn api_urls_yield_their_players_names_and_key() {
        let key = "e91c0d3a-55b2-4e7f-9a18-c3d6f0b2a4e9";
        let input = Input::parse(&format!(
            "https://api.urchin.gg/v3/player/tags?player={OJUGG_DASHED}&key={key}"
        ));
        assert_eq!(
            input.reading,
            Reading::Url {
                identifiers: vec![
                    UrlIdentifier::Player(ojugg()),
                    UrlIdentifier::Key(Uuid::parse_str(key).unwrap()),
                ],
                players_found: 1,
            }
        );
        assert_eq!(
            input.interpretations(),
            vec!["API URL with 1 player identifier and a key"]
        );

        let resolve = Input::parse("https://api.urchin.gg/v3/resolve/oJugg");
        assert_eq!(
            resolve.reading,
            Reading::Url {
                identifiers: vec![UrlIdentifier::Name("ojugg".into())],
                players_found: 1,
            }
        );
    }

    #[test]
    fn session_routes_do_not_read_their_segments_as_names() {
        let input = Input::parse(&format!(
            "https://api.urchin.gg/v3/player/sessions/daily?player={OJUGG}"
        ));
        assert_eq!(
            input.reading,
            Reading::Url {
                identifiers: vec![UrlIdentifier::Player(ojugg())],
                players_found: 1,
            }
        );
    }

    #[test]
    fn long_api_urls_are_capped_and_say_so() {
        let names: Vec<String> = (0..6).map(|index| format!("name{index}")).collect();
        let input = Input::parse(&format!(
            "https://api.urchin.gg/v3/players?names={}",
            names.join(",")
        ));
        let Reading::Url {
            identifiers,
            players_found,
        } = &input.reading
        else {
            panic!("not read as a URL");
        };
        assert_eq!(identifiers.len(), MAX_URL_IDENTIFIERS);
        assert_eq!(*players_found, 6);
        assert_eq!(
            input.interpretations(),
            vec!["API URL with 4 player identifiers (the first 4 of 6 are searched)"]
        );
    }

    #[test]
    fn a_capped_api_url_still_searches_its_key() {
        let key = "e91c0d3a-55b2-4e7f-9a18-c3d6f0b2a4e9";
        let names: Vec<String> = (0..6).map(|index| format!("name{index}")).collect();
        let input = Input::parse(&format!(
            "https://api.urchin.gg/v3/player/tags?name={}&key={key}",
            names.join(",")
        ));
        assert!(
            input
                .lookups()
                .contains(&Lookup::ApiKey(Uuid::parse_str(key).unwrap()))
        );
        assert_eq!(
            input.interpretations(),
            vec!["API URL with 4 player identifiers and a key (the first 4 of 6 are searched)"]
        );
    }

    #[test]
    fn a_raw_name_needs_the_operator_and_a_name_shape() {
        assert_eq!(Input::parse("n:Kestrel_").raw_name(), Some("Kestrel_"));
        assert_eq!(Input::parse("Kestrel_").raw_name(), None);
        assert_eq!(Input::parse("n:has space").raw_name(), None);
        assert_eq!(
            Input::parse("n:Kestrel_").lookups(),
            vec![Lookup::NamedRequests("kestrel_".into())]
        );
        assert_eq!(
            Input::parse("n:x_").interpretations(),
            vec!["forced: raw name", "short name"]
        );
    }

    #[test]
    fn only_mojang_shaped_names_go_to_mojang() {
        assert_eq!(Input::parse("Kestrel_").remote_name(), Some("Kestrel_"));
        assert_eq!(Input::parse("p:Kestrel_").remote_name(), Some("Kestrel_"));
        assert_eq!(Input::parse("a:Kestrel_").remote_name(), None);
        assert_eq!(Input::parse("ab").remote_name(), None);
        assert_eq!(Input::parse("seventeen_chars_x").remote_name(), None);
        assert_eq!(Input::parse("e91c0d3a").remote_name(), Some("e91c0d3a"));
    }

    #[test]
    fn empty_input_reads_as_nothing() {
        assert_eq!(shapes("  \"\" "), Vec::new());
        assert_eq!(Input::parse("p:").interpretations(), vec!["forced: player"]);
    }
}
