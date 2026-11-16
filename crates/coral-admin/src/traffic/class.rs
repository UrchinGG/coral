use std::sync::LazyLock;

use reqlog::Cause;
use serde::{Deserialize, Serialize};

use crate::error::AdminError;

pub const FIRST_ERROR_STATUS: i16 = 400;
pub const LAST_CLIENT_ERROR_STATUS: i16 = 499;
pub const FIRST_SERVER_ERROR_STATUS: i16 = 500;
pub const TOO_MANY_REQUESTS: i16 = 429;
pub const UNMATCHED_ROUTE: &str = "unmatched";

const KNOWN_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub static OUTCOME_SUMS: LazyLock<String> = LazyLock::new(|| {
    Outcome::ALL
        .iter()
        .map(|outcome| {
            format!(
                "{} AS {}",
                requests_where(&outcome.predicate()),
                outcome.column()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Success,
    ClientError,
    CallerLimited,
    UpstreamLimited,
    ServerError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum StatusClass {
    #[serde(rename = "2xx")]
    Success,
    #[serde(rename = "4xx")]
    ClientError,
    #[serde(rename = "5xx")]
    ServerError,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "rate_limited")]
    RateLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteFilter {
    Template(&'static str),
    Unmatched,
    Any(RouteSet),
}

/// Several routes picked at once: a bit per known template, plus unmatched paths.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RouteSet {
    templates: u128,
    unmatched: bool,
}

const _: () = assert!(reqlog::ROUTE_TEMPLATES.len() <= u128::BITS as usize);
const ROUTE_SEPARATOR: char = ',';

impl Outcome {
    pub const ALL: [Self; 5] = [
        Self::Success,
        Self::ClientError,
        Self::CallerLimited,
        Self::UpstreamLimited,
        Self::ServerError,
    ];

    pub fn column(self) -> &'static str {
        match self {
            Self::Success => "c2xx",
            Self::ClientError => "c4xx",
            Self::CallerLimited => "c429_caller",
            Self::UpstreamLimited => "c429_upstream",
            Self::ServerError => "c5xx",
        }
    }

    pub fn predicate(self) -> String {
        let upstream = Cause::UpstreamLimit.code();
        match self {
            Self::Success => format!("status < {FIRST_ERROR_STATUS}"),
            Self::ClientError => format!(
                "status BETWEEN {FIRST_ERROR_STATUS} AND {LAST_CLIENT_ERROR_STATUS} \
                 AND status <> {TOO_MANY_REQUESTS}"
            ),
            Self::CallerLimited => format!("status = {TOO_MANY_REQUESTS} AND cause <> {upstream}"),
            Self::UpstreamLimited => format!("status = {TOO_MANY_REQUESTS} AND cause = {upstream}"),
            Self::ServerError => format!("status >= {FIRST_SERVER_ERROR_STATUS}"),
        }
    }
}

impl StatusClass {
    pub fn predicate(self) -> String {
        match self {
            Self::Success => format!("status < {FIRST_ERROR_STATUS}"),
            Self::ClientError => {
                format!("status BETWEEN {FIRST_ERROR_STATUS} AND {LAST_CLIENT_ERROR_STATUS}")
            }
            Self::ServerError => format!("status >= {FIRST_SERVER_ERROR_STATUS}"),
            Self::Error => format!("status >= {FIRST_ERROR_STATUS}"),
            Self::RateLimited => format!("status = {TOO_MANY_REQUESTS}"),
        }
    }
}

impl RouteFilter {
    /// One route, or several separated by commas.
    pub fn parse(raw: &str) -> Result<Self, AdminError> {
        let picked = raw
            .split(ROUTE_SEPARATOR)
            .map(Self::parse_one)
            .collect::<Result<Vec<_>, _>>()?;
        if let [single] = picked.as_slice() {
            return Ok(*single);
        }
        let mut set = RouteSet::default();
        for route in picked {
            match route {
                Self::Template(template) => set.templates |= 1 << template_index(template),
                Self::Unmatched => set.unmatched = true,
                Self::Any(_) => unreachable!("one route parses to a template or unmatched"),
            }
        }
        Ok(Self::Any(set))
    }

    fn parse_one(raw: &str) -> Result<Self, AdminError> {
        if raw == UNMATCHED_ROUTE {
            return Ok(Self::Unmatched);
        }
        reqlog::ROUTE_TEMPLATES
            .iter()
            .copied()
            .find(|known| *known == raw)
            .map(Self::Template)
            .ok_or_else(|| AdminError::BadRequest(format!("unknown route template {raw}")))
    }
}

impl RouteSet {
    pub fn templates(self) -> Vec<&'static str> {
        reqlog::ROUTE_TEMPLATES
            .iter()
            .enumerate()
            .filter(|(index, _)| self.templates & (1 << index) != 0)
            .map(|(_, template)| *template)
            .collect()
    }

    pub fn unmatched(self) -> bool {
        self.unmatched
    }
}

fn template_index(template: &str) -> usize {
    reqlog::ROUTE_TEMPLATES
        .iter()
        .position(|known| *known == template)
        .expect("parsed templates come from ROUTE_TEMPLATES")
}

pub fn errors_predicate() -> String {
    format!("status >= {FIRST_ERROR_STATUS} AND status <> {TOO_MANY_REQUESTS}")
}

pub fn rate_limited_predicate() -> String {
    StatusClass::RateLimited.predicate()
}

pub fn requests_where(predicate: &str) -> String {
    format!("coalesce(sum(requests) FILTER (WHERE {predicate}), 0)::bigint")
}

pub fn known_method(raw: &str) -> Result<&'static str, AdminError> {
    KNOWN_METHODS
        .iter()
        .copied()
        .find(|method| *method == raw)
        .ok_or_else(|| AdminError::BadRequest(format!("unknown HTTP method {raw}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_sums_name_one_column_per_outcome() {
        for outcome in Outcome::ALL {
            assert!(OUTCOME_SUMS.contains(&format!("AS {}", outcome.column())));
        }
        assert_eq!(OUTCOME_SUMS.matches("FILTER").count(), Outcome::ALL.len());
    }

    #[test]
    fn rate_limits_split_on_the_upstream_cause() {
        let upstream = Cause::UpstreamLimit.code();
        assert_eq!(
            Outcome::UpstreamLimited.predicate(),
            format!("status = 429 AND cause = {upstream}")
        );
        assert_eq!(
            Outcome::CallerLimited.predicate(),
            format!("status = 429 AND cause <> {upstream}")
        );
    }

    #[test]
    fn errors_leave_rate_limits_to_their_own_count() {
        assert_eq!(errors_predicate(), "status >= 400 AND status <> 429");
        assert_eq!(rate_limited_predicate(), "status = 429");
    }

    #[test]
    fn routes_are_known_templates_or_unmatched() {
        assert_eq!(
            RouteFilter::parse("/v3/players").unwrap(),
            RouteFilter::Template("/v3/players")
        );
        assert_eq!(
            RouteFilter::parse(UNMATCHED_ROUTE).unwrap(),
            RouteFilter::Unmatched
        );
        assert_eq!(
            RouteFilter::parse("/v3/nope").unwrap_err().code(),
            "bad_request"
        );
    }

    #[test]
    fn several_routes_become_one_set() {
        let RouteFilter::Any(set) = RouteFilter::parse("/v3/players,unmatched,/v3/tags").unwrap()
        else {
            panic!("several routes should parse to a set");
        };
        assert_eq!(set.templates(), vec!["/v3/players", "/v3/tags"]);
        assert!(set.unmatched());
        assert!(RouteFilter::parse("/v3/players,/v3/nope").is_err());
    }

    #[test]
    fn methods_must_be_known_and_uppercase() {
        assert_eq!(known_method("POST").unwrap(), "POST");
        assert_eq!(known_method("post").unwrap_err().code(), "bad_request");
    }
}
