use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use database::Database;
use reqlog::SubjectSources;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::caller::{self, CallerDetail};
use super::locate::{self, Located, Lookup, StoredDetail, StoredSubject};
use super::neighbors::{self, Direction, Neighbors, RequesterScope};
use super::network::ClientNetwork;
use super::params::{QueryParam, query_params};
use super::rows::{account_id, extracted_subjects, listed_subjects, request_row};
use crate::coverage::Coverage;
use crate::error::AdminError;
use crate::names::{LabelRequest, NameService};
use crate::query::LogRow;
use crate::refs::{Labels, RequestRow, Subject};
use crate::security::json_error;

const HOST_HEADER: &str = "host";
const PUBLIC_API_ORIGIN: &str = "https://api.urchin.gg";
const NEEDS_TIMESTAMP: &str = "needs_timestamp";

#[derive(Debug, Serialize)]
pub struct RequestFull {
    pub request: RequestRow,
    pub url: String,
    pub query_params: Vec<QueryParam>,
    pub headers: Option<BTreeMap<String, String>>,
    pub body: Option<String>,
    pub body_bytes: Option<i32>,
    pub body_complete: bool,
    pub body_json: Option<Value>,
    pub server_error: Option<String>,
    pub detail_state: DetailState,
    pub subjects: Vec<InvolvedPlayer>,
    pub caller: CallerDetail,
    pub neighbors: Neighbors,
    pub labels: Labels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailState {
    Present,
    NotCaptured,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvolvedPlayer {
    #[serde(flatten)]
    pub subject: Subject,
    pub current_name: Option<String>,
}

#[derive(Debug)]
pub enum DetailError {
    NeedsTimestamp(Uuid),
    Admin(AdminError),
}

struct Facts {
    row: LogRow,
    detail: Option<StoredDetail>,
    body_json: Option<Value>,
    subjects: Vec<Subject>,
}

struct Related {
    caller: CallerDetail,
    neighbors: Neighbors,
}

pub async fn read_request(
    db: &Database,
    names: &NameService,
    coverage: &Coverage,
    request_id: Uuid,
    t: Option<DateTime<Utc>>,
) -> Result<RequestFull, DetailError> {
    let probes = match Lookup::of(request_id, t) {
        Lookup::Probes(probes) => probes,
        Lookup::ByDetail(request_id) => vec![
            locate::probe_by_detail(db, request_id)
                .await?
                .ok_or_else(|| not_found_by_detail(request_id))?,
        ],
        Lookup::NeedsTimestamp(request_id) => return Err(DetailError::NeedsTimestamp(request_id)),
    };
    let located = locate::first_located(db, &probes)
        .await?
        .ok_or_else(|| not_found(request_id))?;
    let facts = Facts::from(located);
    let related = related(db, &facts).await?;
    let labels = names.cached(&labels_for(&facts, &related)).await?;
    Ok(facts.into_full(related, labels, coverage))
}

impl Facts {
    fn from(located: Located) -> Self {
        let Located {
            row,
            detail,
            subjects: stored,
        } = located;
        let body = detail.as_ref().and_then(|detail| detail.body.as_deref());
        let body_json = body.and_then(|body| serde_json::from_str(body).ok());
        let extracted = extracted_subjects(&row, body, SubjectSources::All);
        Self {
            subjects: resolved_subjects(extracted, stored),
            body_json,
            detail,
            row,
        }
    }

    fn headers(&self) -> Option<&BTreeMap<String, String>> {
        self.detail.as_ref().map(|detail| &detail.headers.0)
    }

    fn players(&self) -> Vec<Uuid> {
        let mut players: Vec<Uuid> = self
            .subjects
            .iter()
            .filter_map(|subject| subject.player)
            .collect();
        players.sort_unstable();
        players.dedup();
        players
    }

    fn url(&self) -> String {
        let origin = self
            .headers()
            .and_then(|headers| headers.get(HOST_HEADER))
            .map_or_else(
                || PUBLIC_API_ORIGIN.to_owned(),
                |host| format!("https://{host}"),
            );
        match &self.row.query {
            Some(query) => format!("{origin}{}?{query}", self.row.path),
            None => format!("{origin}{}", self.row.path),
        }
    }

    fn into_full(self, related: Related, labels: Labels, coverage: &Coverage) -> RequestFull {
        let network = match self.headers() {
            Some(headers) => {
                ClientNetwork::for_row(&self.row, Some(&ClientNetwork::from_headers(headers)))
            }
            None => ClientNetwork::for_row(&self.row, None),
        };
        let url = self.url();
        let detail_state = detail_state(self.detail.is_some(), self.row.ts, coverage);
        let request = request_row(&self.row, listed_subjects(&self.row), network);
        let query_params = query_params(self.row.query.as_deref(), &self.subjects);
        let subjects = self
            .subjects
            .into_iter()
            .map(|subject| InvolvedPlayer {
                current_name: subject
                    .player
                    .and_then(|player| labels.minecraft.get(&player.simple().to_string()).cloned()),
                subject,
            })
            .collect();
        let (headers, body, body_bytes, body_complete, server_error) = match self.detail {
            Some(detail) => (
                Some(detail.headers.0),
                detail.body,
                Some(detail.body_bytes),
                detail.body_complete,
                detail.server_error,
            ),
            None => (None, None, None, false, None),
        };
        RequestFull {
            request,
            url,
            query_params,
            headers,
            body,
            body_bytes,
            body_complete,
            body_json: self.body_json,
            server_error,
            detail_state,
            subjects,
            caller: related.caller,
            neighbors: related.neighbors,
            labels,
        }
    }
}

impl From<AdminError> for DetailError {
    fn from(error: AdminError) -> Self {
        Self::Admin(error)
    }
}

impl IntoResponse for DetailError {
    fn into_response(self) -> Response {
        match self {
            Self::NeedsTimestamp(request_id) => json_error(
                StatusCode::NOT_FOUND,
                NEEDS_TIMESTAMP,
                &format!(
                    "request {} has a derived id, so it opens only together with its timestamp t",
                    request_id.simple()
                ),
            ),
            Self::Admin(error) => error.into_response(),
        }
    }
}

async fn related(db: &Database, facts: &Facts) -> Result<Related, AdminError> {
    let row = &facts.row;
    let scope = RequesterScope::of(row);
    let players = facts.players();
    let key = caller::presented_key(facts.headers(), row.query.as_deref());
    let token = caller::session_token(facts.headers(), facts.body_json.as_ref());
    let slug = caller::plugin_slug(row.route_template().as_deref(), &row.path);
    let (prev, next, same_player, key, starfish_user, plugin) = tokio::try_join!(
        neighbors::neighbor(db, scope.as_ref(), row, Direction::Older),
        neighbors::neighbor(db, scope.as_ref(), row, Direction::Newer),
        neighbors::same_player_requests(db, &players, row),
        optional(key, |key| caller::key_detail(db, key)),
        optional(token, |token| async move {
            caller::starfish_user(db, &token).await
        }),
        optional(slug, |slug| caller::plugin(db, slug)),
    )?;
    let attribution = row.attribution();
    let requester = attribution.requester();
    Ok(Related {
        caller: CallerDetail {
            account: account_id(&requester).map(|_| requester),
            actor: attribution.actor(),
            key,
            starfish_user: starfish_user.flatten(),
            plugin,
        },
        neighbors: Neighbors {
            prev_same_requester: prev,
            next_same_requester: next,
            same_player_5m: same_player,
        },
    })
}

async fn optional<I, T, F, Fut>(input: Option<I>, read: F) -> Result<Option<T>, AdminError>
where
    F: FnOnce(I) -> Fut,
    Fut: Future<Output = Result<T, AdminError>>,
{
    match input {
        Some(input) => read(input).await.map(Some),
        None => Ok(None),
    }
}

fn labels_for(facts: &Facts, related: &Related) -> LabelRequest {
    let caller = &related.caller;
    let owner = caller.key.as_ref().and_then(|key| key.owner.as_ref());
    let accounts = [
        caller.account.as_ref(),
        caller.actor.as_ref(),
        owner,
        caller.starfish_user.as_ref(),
    ];
    LabelRequest {
        discord: accounts
            .into_iter()
            .flatten()
            .filter_map(account_id)
            .collect(),
        minecraft: facts
            .players()
            .into_iter()
            .map(|player| player.simple().to_string())
            .collect(),
    }
}

fn resolved_subjects(extracted: Vec<Subject>, stored: Vec<StoredSubject>) -> Vec<Subject> {
    let mut unmatched = stored;
    let mut subjects: Vec<Subject> = extracted
        .into_iter()
        .map(|mut subject| {
            if let Some(index) = unmatched.iter().position(|row| row.represents(&subject)) {
                subject.player = subject.player.or(unmatched.swap_remove(index).player);
            }
            subject
        })
        .collect();
    subjects.extend(
        unmatched
            .into_iter()
            .filter_map(StoredSubject::into_subject),
    );
    subjects
}

fn detail_state(captured: bool, ts: DateTime<Utc>, coverage: &Coverage) -> DetailState {
    if captured {
        return DetailState::Present;
    }
    if coverage.detail_since.is_none_or(|since| ts < since) {
        return DetailState::NotCaptured;
    }
    match coverage.detail_from.is_some_and(|from| ts < from) {
        true => DetailState::Expired,
        false => DetailState::NotCaptured,
    }
}

fn not_found(request_id: Uuid) -> AdminError {
    AdminError::NotFound(format!(
        "no request {} in the request log",
        request_id.simple()
    ))
}

fn not_found_by_detail(request_id: Uuid) -> AdminError {
    AdminError::NotFound(format!(
        "no request {} has stored detail; an id derived for a row logged before request ids \
         existed opens only together with its timestamp t",
        request_id.simple()
    ))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use reqlog::Form;

    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn detail_states_separate_not_captured_from_expired() {
        let coverage = Coverage {
            detail_since: Some(at(14)),
            detail_from: Some(at(16)),
            ..Coverage::default()
        };
        assert_eq!(detail_state(true, at(10), &coverage), DetailState::Present);
        assert_eq!(
            detail_state(false, at(13), &coverage),
            DetailState::NotCaptured
        );
        assert_eq!(detail_state(false, at(15), &coverage), DetailState::Expired);
        assert_eq!(
            detail_state(false, at(17), &coverage),
            DetailState::NotCaptured
        );
        assert_eq!(
            detail_state(false, at(17), &Coverage::default()),
            DetailState::NotCaptured
        );
    }

    #[test]
    fn stored_rows_resolve_extracted_names_and_add_what_extraction_missed() {
        let player = Uuid::parse_str(OJUGG).unwrap();
        let other = Uuid::from_u128(7);
        let extracted = vec![Subject {
            player: None,
            name: Some("ojugg".into()),
            raw: "oJugg".into(),
            form: Form::Name,
        }];
        let stored = vec![
            StoredSubject {
                form: Form::Name.code(),
                player: Some(player),
                name: Some("ojugg".into()),
            },
            StoredSubject {
                form: Form::Body.code(),
                player: Some(other),
                name: None,
            },
        ];
        let subjects = resolved_subjects(extracted, stored);
        assert_eq!(subjects.len(), 2);
        assert_eq!(subjects[0].player, Some(player));
        assert_eq!(subjects[0].raw, "oJugg");
        assert_eq!(subjects[1].player, Some(other));
        assert_eq!(subjects[1].raw, other.simple().to_string());
    }
}
