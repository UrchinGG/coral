use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use percent_encoding::percent_decode_str;
use serde_json::value::RawValue;
use uuid::Uuid;

use crate::routes::{SubjectSource, is_mapped_route, route_subjects};
use crate::status::METHOD_NOT_ALLOWED;

const UUID_HEX_LEN: usize = 32;
const MAX_NAME_LEN: usize = 25;
const PLAYER_QUERY_FIELDS: &[&str] = &["player", "uuid", "name"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum Form {
    Undashed = 1,
    Dashed = 2,
    Name = 3,
    Display = 4,
    Path = 5,
    Body = 6,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub form: Form,
    pub player: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectSources {
    All,
    Query,
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct RequestParts<'a> {
    pub method: &'a str,
    pub status: u16,
    pub route: Option<&'a str>,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub body: Option<&'a str>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Extraction {
    pub subjects: Vec<Subject>,
    pub unmapped: bool,
}

type BodyFields<'a> = HashMap<String, &'a RawValue>;

#[derive(Debug, Clone, Copy)]
enum Origin {
    Query,
    Path,
    Body,
}

impl Form {
    pub fn code(self) -> i16 {
        self as i16
    }

    pub fn resolves_by_name(self) -> bool {
        self != Self::Display
    }
}

impl Subject {
    fn player(player: Uuid, form: Form) -> Self {
        Self {
            form,
            player: Some(player),
            name: None,
        }
    }

    fn name(name: &str, form: Form) -> Self {
        Self {
            form,
            player: None,
            name: Some(name.to_ascii_lowercase()),
        }
    }
}

impl SubjectSources {
    pub fn enabled(self) -> bool {
        self != Self::None
    }

    pub fn gap_features(self) -> &'static [&'static str] {
        match self {
            Self::All => &[],
            Self::Query => &["body_subjects"],
            Self::None => &["subjects", "body_subjects"],
        }
    }

    fn admits(self, source: SubjectSource) -> bool {
        match self {
            Self::All => true,
            Self::Query => !source.reads_body(),
            Self::None => false,
        }
    }
}

impl FromStr for SubjectSources {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim() {
            "all" => Ok(Self::All),
            "query" => Ok(Self::Query),
            "none" => Ok(Self::None),
            other => Err(format!(
                "unknown subject source setting {other:?}; expected all, query or none"
            )),
        }
    }
}

impl Origin {
    fn uuid_form(self, raw: &str) -> Form {
        match self {
            Self::Query if raw.contains('-') => Form::Dashed,
            Self::Query => Form::Undashed,
            Self::Path => Form::Path,
            Self::Body => Form::Body,
        }
    }

    fn name_form(self) -> Form {
        match self {
            Self::Query => Form::Name,
            Self::Path => Form::Path,
            Self::Body => Form::Body,
        }
    }
}

pub fn extract(request: &RequestParts, sources: SubjectSources) -> Extraction {
    let Some(route) = request.route else {
        return Extraction::default();
    };
    let Some(mapping) = route_subjects(request.method, route) else {
        return Extraction {
            subjects: Vec::new(),
            unmapped: is_unmapped(request, route),
        };
    };
    let body = mapping
        .sources
        .iter()
        .any(|source| source.reads_body() && sources.admits(*source))
        .then(|| parse_body(request.body))
        .flatten();
    let subjects = mapping
        .sources
        .iter()
        .filter(|source| sources.admits(**source))
        .flat_map(|source| source_subjects(*source, request, body.as_ref()))
        .collect();
    Extraction {
        subjects: deduplicate(subjects),
        unmapped: false,
    }
}

fn source_subjects(
    source: SubjectSource,
    request: &RequestParts,
    body: Option<&BodyFields>,
) -> Vec<Subject> {
    match source {
        SubjectSource::Query(field) => query_value(request.query, field)
            .and_then(|value| player_subject(&value, Origin::Query))
            .into_iter()
            .collect(),
        SubjectSource::QueryDisplay(field) => query_value(request.query, field)
            .and_then(|value| name_subject(value.trim(), Form::Display))
            .into_iter()
            .collect(),
        SubjectSource::PathTail => path_tail(request.path)
            .and_then(|value| player_subject(&value, Origin::Path))
            .into_iter()
            .collect(),
        SubjectSource::BodyArray { field, max } => body_array(body, field, max),
        SubjectSource::BodyField(field) => body
            .and_then(|fields| fields.get(field))
            .and_then(|raw| body_text(raw))
            .and_then(|value| player_subject(&value, Origin::Body))
            .into_iter()
            .collect(),
    }
}

fn body_array(body: Option<&BodyFields>, field: &str, max: usize) -> Vec<Subject> {
    let Some(items) = body
        .and_then(|fields| fields.get(field))
        .and_then(|raw| serde_json::from_str::<Vec<&RawValue>>(raw.get()).ok())
    else {
        return Vec::new();
    };
    if items.len() > max {
        return Vec::new();
    }
    items
        .into_iter()
        .filter_map(body_text)
        .filter_map(|value| player_subject(&value, Origin::Body))
        .collect()
}

fn body_text(raw: &RawValue) -> Option<String> {
    serde_json::from_str(raw.get()).ok()
}

fn player_subject(raw: &str, origin: Origin) -> Option<Subject> {
    let value = raw.trim();
    match parse_uuid(value) {
        Some(player) => Some(Subject::player(player, origin.uuid_form(value))),
        None => name_subject(value, origin.name_form()),
    }
}

fn name_subject(value: &str, form: Form) -> Option<Subject> {
    is_player_name(value).then(|| Subject::name(value, form))
}

fn parse_uuid(value: &str) -> Option<Uuid> {
    let hex: String = value.chars().filter(|c| *c != '-').collect();
    (hex.len() == UUID_HEX_LEN)
        .then(|| Uuid::try_parse(&hex).ok())
        .flatten()
}

fn is_player_name(value: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn query_value(query: Option<&str>, field: &str) -> Option<String> {
    form_urlencoded::parse(query?.as_bytes())
        .find(|(key, _)| key == field)
        .map(|(_, value)| value.into_owned())
}

fn path_tail(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    Some(percent_decode_str(tail).decode_utf8_lossy().into_owned())
}

fn parse_body(body: Option<&str>) -> Option<BodyFields<'_>> {
    serde_json::from_str(body?).ok()
}

fn is_unmapped(request: &RequestParts, route: &str) -> bool {
    request.status != METHOD_NOT_ALLOWED
        && !is_mapped_route(route)
        && carries_player_query(request.query)
}

fn carries_player_query(query: Option<&str>) -> bool {
    query.is_some_and(|query| {
        form_urlencoded::parse(query.as_bytes())
            .any(|(key, _)| PLAYER_QUERY_FIELDS.contains(&key.as_ref()))
    })
}

fn deduplicate(subjects: Vec<Subject>) -> Vec<Subject> {
    let mut seen = HashSet::new();
    subjects
        .into_iter()
        .filter(|subject| seen.insert((subject.player, subject.name.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_BATCH_UUIDS;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";
    const OJUGG_DASHED: &str = "94589c87-999b-4a7a-b3ad-d2ae50711e64";

    fn ojugg() -> Uuid {
        Uuid::parse_str(OJUGG).unwrap()
    }

    fn get(route: &str, query: &str) -> Extraction {
        request("GET", route, route, Some(query), None)
    }

    fn request(
        method: &str,
        route: &str,
        path: &str,
        query: Option<&str>,
        body: Option<&str>,
    ) -> Extraction {
        extract(
            &RequestParts {
                method,
                status: 200,
                route: Some(route),
                path,
                query,
                body,
            },
            SubjectSources::All,
        )
    }

    fn player(form: Form) -> Subject {
        Subject::player(ojugg(), form)
    }

    fn name(value: &str, form: Form) -> Subject {
        Subject::name(value, form)
    }

    #[test]
    fn query_player_accepts_undashed_dashed_and_names() {
        assert_eq!(
            get("/v3/player/tags", &format!("player={OJUGG}")).subjects,
            vec![player(Form::Undashed)]
        );
        assert_eq!(
            get("/v3/player/tags", &format!("player={OJUGG_DASHED}")).subjects,
            vec![player(Form::Dashed)]
        );
        assert_eq!(
            get("/v3/player/tags", "player=oJugg").subjects,
            vec![name("ojugg", Form::Name)]
        );
    }

    #[test]
    fn uppercase_and_irregularly_dashed_uuids_become_players() {
        let upper = OJUGG_DASHED.to_uppercase();
        assert_eq!(
            get("/v3/player/profile", &format!("uuid={upper}")).subjects,
            vec![player(Form::Dashed)]
        );
        assert_eq!(
            get("/v3/player/face", "uuid=94589c87999b4a7a-b3add2ae50711e64").subjects,
            vec![player(Form::Dashed)]
        );
    }

    #[test]
    fn every_player_query_field_is_read_once() {
        let extraction = get(
            "/v3/player/tags",
            &format!("name=Technoblade&uuid={OJUGG}&player=oJugg&player=Other"),
        );
        assert_eq!(
            extraction.subjects,
            vec![
                name("ojugg", Form::Name),
                player(Form::Undashed),
                name("technoblade", Form::Name),
            ]
        );
    }

    #[test]
    fn query_values_are_form_decoded_and_trimmed() {
        assert_eq!(
            get("/v3/player/tags", "player=%20oJugg+").subjects,
            vec![name("ojugg", Form::Name)]
        );
    }

    #[test]
    fn values_that_are_neither_uuids_nor_names_are_dropped() {
        for query in [
            "player=%C2%A7edoorman",
            "player={{name}}",
            "player=",
            "player=a_name_that_is_far_too_long_to_be_valid",
            "player=94589c87999b4a7ab3add2ae50711e6",
        ] {
            assert!(get("/v3/player/tags", query).subjects.is_empty(), "{query}");
        }
    }

    #[test]
    fn duplicates_within_a_request_are_removed() {
        assert_eq!(
            get(
                "/v3/hypixel/player",
                &format!("player={OJUGG}&uuid={OJUGG_DASHED}")
            )
            .subjects,
            vec![player(Form::Undashed)]
        );
    }

    #[test]
    fn guild_lookups_only_read_the_player_parameter() {
        assert_eq!(
            get(
                "/v3/hypixel/guild",
                &format!("name=SomeGuild&player={OJUGG}")
            )
            .subjects,
            vec![player(Form::Undashed)]
        );
        assert!(
            get("/v3/hypixel/guild", "name=SomeGuild")
                .subjects
                .is_empty()
        );
    }

    #[test]
    fn session_routes_read_player_for_every_method() {
        for method in ["GET", "POST", "PATCH", "DELETE"] {
            let extraction = request(
                method,
                "/v3/player/sessions/markers/{name}",
                "/v3/player/sessions/markers/Bedwars",
                Some("player=oJugg"),
                None,
            );
            assert_eq!(extraction.subjects, vec![name("ojugg", Form::Name)]);
        }
    }

    #[test]
    fn tag_writes_read_the_target_query() {
        let extraction = request(
            "POST",
            "/v3/tags",
            "/v3/tags",
            Some(&format!("uuid={OJUGG}")),
            Some(r#"{"type":"sniper","reason":"Technoblade"}"#),
        );
        assert_eq!(extraction.subjects, vec![player(Form::Undashed)]);
    }

    #[test]
    fn resolve_reads_the_decoded_path_tail() {
        let resolve = |path: &str| request("GET", "/v3/resolve/{identifier}", path, None, None);
        assert_eq!(
            resolve("/v3/resolve/oJugg").subjects,
            vec![name("ojugg", Form::Path)]
        );
        assert_eq!(
            resolve(&format!("/v3/resolve/{OJUGG_DASHED}")).subjects,
            vec![player(Form::Path)]
        );
        assert_eq!(
            resolve("/v3/resolve/%20oJugg").subjects,
            vec![name("ojugg", Form::Path)]
        );
        assert!(resolve("/v3/resolve/%C2%A7e").subjects.is_empty());
    }

    #[test]
    fn batch_bodies_yield_every_uuid() {
        let other = "0123456789abcdef0123456789abcdef";
        let body = format!(r#"{{"uuids":["{OJUGG}","{OJUGG_DASHED}","{other}","bad entry"]}}"#);
        let extraction = request("POST", "/v3/players", "/v3/players", None, Some(&body));
        assert_eq!(
            extraction.subjects,
            vec![
                player(Form::Body),
                Subject::player(Uuid::parse_str(other).unwrap(), Form::Body),
            ]
        );
    }

    #[test]
    fn batch_bodies_over_the_route_maximum_yield_no_subjects() {
        let uuids: Vec<String> = (0..=MAX_BATCH_UUIDS)
            .map(|i| format!("\"{i:032x}\""))
            .collect();
        let body = format!(r#"{{"uuids":[{}]}}"#, uuids.join(","));
        let extraction = request("POST", "/v3/players", "/v3/players", None, Some(&body));
        assert!(extraction.subjects.is_empty());

        let body = format!(r#"{{"uuids":[{}]}}"#, uuids[..MAX_BATCH_UUIDS].join(","));
        let extraction = request("POST", "/v3/players", "/v3/players", None, Some(&body));
        assert_eq!(extraction.subjects.len(), MAX_BATCH_UUIDS);
    }

    #[test]
    fn body_fields_are_read_past_unknown_and_non_string_values() {
        let other = "0123456789abcdef0123456789abcdef";
        let padding = "x".repeat(4_096);
        let body = format!(
            r#"{{"padding":{{"deep":["{padding}"]}},"uuids":[7,"{OJUGG}",null,"\u0030{}"]}}"#,
            &other[1..]
        );
        let extraction = request("POST", "/v3/players", "/v3/players", None, Some(&body));
        assert_eq!(
            extraction.subjects,
            vec![
                player(Form::Body),
                Subject::player(Uuid::parse_str(other).unwrap(), Form::Body),
            ]
        );

        let body = format!(r#"{{"uuid":5,"username":"oJugg","extra":{{"uuid":"{OJUGG}"}}}}"#);
        let extraction = request(
            "POST",
            "/v3/verify/codes",
            "/v3/verify/codes",
            None,
            Some(&body),
        );
        assert_eq!(extraction.subjects, vec![name("ojugg", Form::Body)]);
    }

    #[test]
    fn unparseable_bodies_yield_no_subjects() {
        let extraction = request(
            "POST",
            "/v3/players",
            "/v3/players",
            None,
            Some(r#"{"uuids":["#),
        );
        assert!(extraction.subjects.is_empty());
    }

    #[test]
    fn verify_bodies_yield_uuid_and_username() {
        let body = format!(r#"{{"code":"123456","uuid":"{OJUGG}","username":"oJugg"}}"#);
        let extraction = request(
            "POST",
            "/v3/verify/codes",
            "/v3/verify/codes",
            None,
            Some(&body),
        );
        assert_eq!(
            extraction.subjects,
            vec![player(Form::Body), name("ojugg", Form::Body)]
        );
    }

    #[test]
    fn cubelify_reads_uuid_and_display_name() {
        let extraction = get(
            "/v3/cubelify",
            &format!("uuid={OJUGG_DASHED}&key=k&name=SomeNick&sources=x"),
        );
        assert_eq!(
            extraction.subjects,
            vec![player(Form::Dashed), name("somenick", Form::Display)]
        );
        let extraction = get(
            "/v3/cubelify",
            &format!("uuid={OJUGG}&key=k&name=%C2%A7eDoorman"),
        );
        assert_eq!(extraction.subjects, vec![player(Form::Undashed)]);
    }

    #[test]
    fn display_names_are_not_resolved_by_name() {
        assert!(!Form::Display.resolves_by_name());
        assert!(Form::Name.resolves_by_name());
        assert!(Form::Body.resolves_by_name());
    }

    #[test]
    fn unmatched_requests_yield_nothing() {
        let extraction = extract(
            &RequestParts {
                method: "GET",
                status: 404,
                route: None,
                path: "/player/oJugg",
                query: Some("player=oJugg"),
                body: None,
            },
            SubjectSources::All,
        );
        assert_eq!(extraction, Extraction::default());
    }

    #[test]
    fn matched_routes_without_a_mapping_flag_player_queries() {
        let extraction = get("/v3/guild/sessions/daily", "guild=abc&name=oJugg");
        assert!(extraction.subjects.is_empty());
        assert!(extraction.unmapped);
        assert!(!get("/v3/guild/sessions/daily", "guild=abc").unmapped);
        assert!(!get("/v3/player/tags", "player=oJugg").unmapped);
    }

    #[test]
    fn head_lookups_yield_the_get_subjects() {
        let extraction = request(
            "HEAD",
            "/v3/player/tags",
            "/v3/player/tags",
            Some("player=oJugg"),
            None,
        );
        assert_eq!(extraction.subjects, vec![name("ojugg", Form::Name)]);
        assert!(!extraction.unmapped);
    }

    #[test]
    fn other_methods_on_mapped_routes_are_not_unmapped() {
        let extraction = request("PUT", "/v3/tags", "/v3/tags", Some("player=oJugg"), None);
        assert!(extraction.subjects.is_empty());
        assert!(!extraction.unmapped);
    }

    #[test]
    fn rejected_methods_are_not_unmapped() {
        let parts = RequestParts {
            method: "PUT",
            status: METHOD_NOT_ALLOWED,
            route: Some("/v3/guild/sessions/daily"),
            path: "/v3/guild/sessions/daily",
            query: Some("name=oJugg"),
            body: None,
        };
        assert!(!extract(&parts, SubjectSources::All).unmapped);
    }

    #[test]
    fn query_setting_drops_body_sources() {
        let body = format!(r#"{{"uuids":["{OJUGG}"]}}"#);
        let parts = RequestParts {
            method: "POST",
            status: 200,
            route: Some("/v3/players"),
            path: "/v3/players",
            query: None,
            body: Some(&body),
        };
        assert!(extract(&parts, SubjectSources::Query).subjects.is_empty());
        let parts = RequestParts {
            method: "GET",
            status: 200,
            route: Some("/v3/player/tags"),
            path: "/v3/player/tags",
            query: Some("player=oJugg"),
            body: None,
        };
        assert_eq!(extract(&parts, SubjectSources::Query).subjects.len(), 1);
        assert!(extract(&parts, SubjectSources::None).subjects.is_empty());
    }

    #[test]
    fn subject_sources_parse_from_the_environment_value() {
        assert_eq!("all".parse(), Ok(SubjectSources::All));
        assert_eq!(" query ".parse(), Ok(SubjectSources::Query));
        assert_eq!("none".parse(), Ok(SubjectSources::None));
        assert!("body".parse::<SubjectSources>().is_err());
    }
}
