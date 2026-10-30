use database::Database;
use reqlog::{ROUTE_SUBJECTS, RequestParts, SubjectSources};
use uuid::Uuid;

use super::network::{ClientNetwork, ClientNetworks};
use crate::coverage::Coverage;
use crate::error::AdminError;
use crate::names::{LabelRequest, NameService};
use crate::query::{LogPage, LogRow};
use crate::refs::{EntityRef, Page, RequestRow, Subject};

pub struct RequestRows<'a> {
    pub db: &'a Database,
    pub names: &'a NameService,
    pub coverage: &'a Coverage,
}

impl RequestRows<'_> {
    pub async fn page(&self, page: LogPage) -> Result<Page<RequestRow>, AdminError> {
        let subjects: Vec<Vec<Subject>> = page.rows.iter().map(listed_subjects).collect();
        let wanted = labels_for(&page.rows, subjects.iter().flatten());
        let (networks, labels) = tokio::try_join!(
            ClientNetworks::read(self.db, &page.rows, self.coverage),
            self.names.cached(&wanted),
        )?;
        let items = page
            .rows
            .iter()
            .zip(subjects)
            .map(|(row, subjects)| request_row(row, subjects, networks.of(row)))
            .collect();
        Ok(Page {
            items,
            next_cursor: page.next_cursor,
            scanned_to: page.scanned_to,
            clamped_from: page.clamped_from,
            labels,
        })
    }
}

pub fn request_row(row: &LogRow, subjects: Vec<Subject>, network: ClientNetwork) -> RequestRow {
    let attribution = row.attribution();
    RequestRow {
        request_id: row.request_key,
        ts: row.ts,
        method: row.method.clone(),
        route: row.route_template(),
        path: row.path.clone(),
        query: row.query.clone(),
        status: row.status,
        outcome_status: row.outcome_status,
        cause: row.cause(),
        error: row.error.clone(),
        latency_ms: row.latency_ms,
        key_kind: attribution.key_kind,
        key_prefix: row.key_prefix.clone(),
        requester: attribution.requester(),
        actor: attribution.actor(),
        ip: row.ip.clone().map(|ip| EntityRef::Ip {
            ip,
            country: network.country.clone(),
            proxied: network.proxied,
        }),
        country: network.country,
        user_agent: row.user_agent.clone(),
        body_subjects: body_subjects(row, &subjects),
        subjects,
        matched: row.matched(),
    }
}

pub fn listed_subjects(row: &LogRow) -> Vec<Subject> {
    extracted_subjects(row, None, SubjectSources::Query)
}

pub fn extracted_subjects(
    row: &LogRow,
    body: Option<&str>,
    sources: SubjectSources,
) -> Vec<Subject> {
    let route = row.route_template();
    let parts = RequestParts {
        method: &row.method,
        status: row.semantic_status() as u16,
        route: route.as_deref(),
        path: &row.path,
        query: row.query.as_deref(),
        body,
    };
    reqlog::extract(&parts, sources)
        .subjects
        .into_iter()
        .map(|subject| Subject {
            player: subject.player,
            name: subject.name,
            raw: subject.raw,
            form: subject.form,
        })
        .collect()
}

pub fn labels_for<'a>(
    rows: impl IntoIterator<Item = &'a LogRow>,
    subjects: impl Iterator<Item = &'a Subject>,
) -> LabelRequest {
    let refs: Vec<EntityRef> = rows
        .into_iter()
        .flat_map(|row| [Some(row.requester()), row.actor()])
        .flatten()
        .collect();
    LabelRequest {
        discord: refs.iter().filter_map(account_id).collect(),
        minecraft: refs
            .iter()
            .filter_map(player_uuid)
            .chain(subjects.filter_map(|subject| subject.player))
            .map(|player| player.simple().to_string())
            .collect(),
    }
}

pub fn account_id(entity: &EntityRef) -> Option<i64> {
    match entity {
        EntityRef::Account { discord_id, .. } => Some(*discord_id),
        _ => None,
    }
}

pub fn player_uuid(entity: &EntityRef) -> Option<Uuid> {
    match entity {
        EntityRef::Player { uuid } => Some(*uuid),
        _ => None,
    }
}

fn body_subjects(row: &LogRow, listed: &[Subject]) -> i16 {
    if !row
        .route_template()
        .is_some_and(|route| reads_body_subjects(&route))
    {
        return 0;
    }
    row.subjects
        .unwrap_or_default()
        .saturating_sub(listed.len() as i16)
        .max(0)
}

fn reads_body_subjects(route: &str) -> bool {
    ROUTE_SUBJECTS
        .iter()
        .filter(|entry| entry.templates.contains(&route))
        .any(|entry| entry.sources.iter().any(|source| source.reads_body()))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn row(method: &str, route: &str, stored: i16) -> LogRow {
        LogRow {
            ts: Utc::now(),
            request_key: Uuid::nil(),
            legacy: false,
            method: method.into(),
            route: Some(route.into()),
            path: route.into(),
            query: None,
            status: 200,
            outcome_status: None,
            cause: None,
            error: None,
            latency_ms: 1,
            key_kind: None,
            key_prefix: None,
            member_id: None,
            member_discord_id: None,
            actor_id: None,
            actor_member_id: None,
            actor_uuid: None,
            ip: None,
            country: None,
            user_agent: None,
            subjects: Some(stored),
            matched: None,
        }
    }

    #[test]
    fn only_routes_that_read_bodies_have_body_subjects() {
        assert_eq!(body_subjects(&row("POST", "/v3/players", 3), &[]), 3);
        assert_eq!(body_subjects(&row("GET", "/v3/player/tags", 3), &[]), 0);
    }
}
