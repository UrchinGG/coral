use std::collections::HashSet;

use chrono::{DateTime, Utc};
use reqlog::{Form, RequestParts, SubjectSources};
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::query::{LogQuery, LogRow, RequestFilter};
use crate::refs::{EntityRef, Labels, Page, RequestRow, Subject};
use crate::routes::ref_labels::label_request;
use crate::state::AppState;

pub const CF_WORKER_HEADER: &str = "cf-worker";

pub enum StreamScope {
    Member(i64),
    Actor(i64),
    Ip(String),
    KeyPrefix(String),
}

impl StreamScope {
    fn narrow(self, mut filter: RequestFilter) -> Result<RequestFilter, AdminError> {
        if filter.q.is_some() {
            return Err(AdminError::BadRequest(
                "free text travels only in the body of POST /requests/query".into(),
            ));
        }
        let scoped = [
            filter.member.is_some(),
            filter.actor.is_some(),
            filter.ip.is_some(),
            filter.key_prefix.is_some(),
            filter.player.is_some(),
        ];
        if scoped.into_iter().any(|set| set) {
            return Err(AdminError::BadRequest(
                "this stream is fixed to its page; member, actor, ip, key_prefix and player cannot be set"
                    .into(),
            ));
        }
        match self {
            Self::Member(member_id) => filter.member = Some(member_id),
            Self::Actor(discord_id) => filter.actor = Some(discord_id.to_string()),
            Self::Ip(ip) => filter.ip = Some(ip),
            Self::KeyPrefix(prefix) => filter.key_prefix = Some(prefix),
        }
        Ok(filter)
    }
}

pub async fn request_stream(
    state: &AppState,
    scope: StreamScope,
    filter: RequestFilter,
) -> Result<Page<RequestRow>, AdminError> {
    let filter = scope.narrow(filter)?;
    let coverage = state.coverage.current();
    let page = LogQuery::new(&state.db, &coverage, &state.players, &state.scan_rates)
        .run(filter)
        .await?;
    let proxied = proxied_requests(state, &page.rows).await?;
    let items: Vec<RequestRow> = page
        .rows
        .iter()
        .map(|row| request_row(row, &proxied))
        .collect();
    let labels = row_labels(state, &items).await?;
    Ok(Page {
        items,
        next_cursor: page.next_cursor,
        scanned_to: page.scanned_to,
        clamped_from: page.clamped_from,
        labels,
    })
}

pub fn request_row(row: &LogRow, proxied: &HashSet<Uuid>) -> RequestRow {
    let route = row.route_template();
    let subjects = listed_subjects(row, route.as_deref());
    let body_subjects = row.subjects.map_or(0, |stored| {
        stored.saturating_sub(subjects.len() as i16).max(0)
    });
    RequestRow {
        request_id: row.request_key,
        ts: row.ts,
        method: row.method.clone(),
        route,
        path: row.path.clone(),
        query: row.query.clone(),
        status: row.status,
        outcome_status: row.outcome_status,
        cause: row.cause(),
        error: row.error.clone(),
        latency_ms: row.latency_ms,
        key_kind: row.key_kind(),
        key_prefix: row.key_prefix.clone(),
        requester: row.requester(),
        actor: row.actor(),
        ip: row.ip.as_ref().map(|ip| EntityRef::Ip {
            ip: ip.clone(),
            country: row.country.clone(),
            proxied: proxied.contains(&row.request_key),
        }),
        country: row.country.clone(),
        user_agent: row.user_agent.clone(),
        subjects,
        body_subjects,
        matched: row.matched(),
    }
}

async fn row_labels(state: &AppState, rows: &[RequestRow]) -> Result<Labels, AdminError> {
    let players: Vec<EntityRef> = rows
        .iter()
        .flat_map(|row| row.subjects.iter().filter_map(|subject| subject.player))
        .map(|uuid| EntityRef::Player { uuid })
        .collect();
    let people = rows
        .iter()
        .flat_map(|row| std::iter::once(&row.requester).chain(row.actor.as_ref()));
    state
        .names
        .cached(&label_request(people.chain(&players)))
        .await
}

async fn proxied_requests(state: &AppState, rows: &[LogRow]) -> Result<HashSet<Uuid>, AdminError> {
    let detail_since = state.coverage.current().detail_since;
    let captured: Vec<&LogRow> = rows
        .iter()
        .filter(|row| row.ip.is_some() && detail_since.is_some_and(|since| row.ts >= since))
        .collect();
    let stamps: Vec<DateTime<Utc>> = captured.iter().map(|row| row.ts).collect();
    let (Some(earliest), Some(latest)) =
        (stamps.iter().min().copied(), stamps.iter().max().copied())
    else {
        return Ok(HashSet::new());
    };
    let ids: Vec<Uuid> = captured.iter().map(|row| row.request_key).collect();
    let proxied: Vec<Uuid> = sqlx::query_scalar(
        "SELECT request_id FROM api_request_detail
         WHERE ts >= $3 AND ts <= $4 AND ts = ANY($1) AND request_id = ANY($2) AND headers ? $5",
    )
    .bind(&stamps)
    .bind(&ids)
    .bind(earliest)
    .bind(latest)
    .bind(CF_WORKER_HEADER)
    .fetch_all(state.db.pool())
    .await
    .context("read which requests came through a Cloudflare Worker")?;
    Ok(proxied.into_iter().collect())
}

fn listed_subjects(row: &LogRow, route: Option<&str>) -> Vec<Subject> {
    let parts = RequestParts {
        method: &row.method,
        status: row.semantic_status().max(0) as u16,
        route,
        path: &row.path,
        query: row.query.as_deref(),
        body: None,
    };
    reqlog::extract(&parts, SubjectSources::Query)
        .subjects
        .into_iter()
        .map(|subject| Subject {
            raw: raw_subject(&subject),
            player: subject.player,
            name: subject.name,
            form: subject.form,
        })
        .collect()
}

fn raw_subject(subject: &reqlog::Subject) -> String {
    match (subject.form, subject.player, &subject.name) {
        (Form::Dashed, Some(player), _) => player.hyphenated().to_string(),
        (_, _, Some(name)) => name.clone(),
        (_, Some(player), None) => player.simple().to_string(),
        (_, None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Cursor;

    #[test]
    fn a_scoped_stream_refuses_text_and_other_scopes() {
        let text = RequestFilter {
            q: Some("secret".into()),
            ..RequestFilter::default()
        };
        assert_eq!(
            StreamScope::Member(1).narrow(text).unwrap_err().code(),
            "bad_request"
        );
        let other = RequestFilter {
            ip: Some("10.0.0.1".into()),
            ..RequestFilter::default()
        };
        assert_eq!(
            StreamScope::Member(1).narrow(other).unwrap_err().code(),
            "bad_request"
        );
    }

    #[test]
    fn a_scoped_stream_keeps_paging_and_sets_its_scope() {
        let cursor = Cursor::before(Utc::now());
        let filter = RequestFilter {
            cursor: Some(cursor),
            ..RequestFilter::default()
        };
        let narrowed = StreamScope::Actor(42).narrow(filter).unwrap();
        assert_eq!(narrowed.actor.as_deref(), Some("42"));
        assert_eq!(narrowed.cursor, Some(cursor));
        let by_key = StreamScope::KeyPrefix("1a2b3c4d".into())
            .narrow(RequestFilter::default())
            .unwrap();
        assert_eq!(by_key.key_prefix.as_deref(), Some("1a2b3c4d"));
    }

    fn tags_lookup(query: &str) -> LogRow {
        LogRow {
            ts: Utc::now(),
            request_key: Uuid::nil(),
            legacy: false,
            method: "GET".into(),
            route: Some("/v3/player/tags".into()),
            path: "/v3/player/tags".into(),
            query: Some(query.into()),
            status: 200,
            outcome_status: None,
            cause: Some(0),
            error: None,
            latency_ms: 4,
            key_kind: Some("personal".into()),
            key_prefix: Some("1a2b3c4d".into()),
            member_id: Some(7),
            member_discord_id: Some(354_598_337_325_826_048),
            actor_id: None,
            actor_member_id: None,
            ip: Some("203.0.113.9".into()),
            country: Some("NL".into()),
            user_agent: None,
            subjects: Some(3),
            matched: None,
        }
    }

    #[test]
    fn query_subjects_keep_the_form_they_arrived_in() {
        let row = tags_lookup("player=94589c87-999b-4a7a-b3ad-d2ae50711e64");
        let converted = request_row(&row, &HashSet::new());
        assert_eq!(converted.subjects.len(), 1);
        assert_eq!(
            converted.subjects[0].raw,
            "94589c87-999b-4a7a-b3ad-d2ae50711e64"
        );
        assert_eq!(converted.body_subjects, 2);
        assert_eq!(
            converted.ip,
            Some(EntityRef::Ip {
                ip: "203.0.113.9".into(),
                country: Some("NL".into()),
                proxied: false
            })
        );
    }

    #[test]
    fn proxied_requests_carry_the_worker_flag_on_their_ip() {
        let row = tags_lookup("player=oJugg");
        let converted = request_row(&row, &HashSet::from([row.request_key]));
        assert!(matches!(
            converted.ip,
            Some(EntityRef::Ip { proxied: true, .. })
        ));
        assert_eq!(converted.subjects[0].raw, "ojugg");
    }
}
