use std::collections::BTreeMap;

use chrono::{DateTime, TimeDelta, Utc};
use database::Database;
use reqlog::Form;
use sqlx::FromRow;
use sqlx::types::Json;
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::query::{LogRow, log_row_select};
use crate::refs::Subject;

pub const TIME_ORDERED_PROBE: TimeDelta = TimeDelta::seconds(1);

const EXACT_INSTANT: TimeDelta = TimeDelta::microseconds(1);
const RANDOM_VERSION: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub request_id: Uuid,
    pub from: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Probes(Vec<Probe>),
    ByDetail(Uuid),
    NeedsTimestamp(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct StoredDetail {
    pub ts: DateTime<Utc>,
    pub headers: Json<BTreeMap<String, String>>,
    pub body: Option<String>,
    pub body_bytes: i32,
    pub body_complete: bool,
    pub server_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct StoredSubject {
    pub form: i16,
    pub player: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub row: LogRow,
    pub detail: Option<StoredDetail>,
    pub subjects: Vec<StoredSubject>,
}

impl Lookup {
    pub fn of(request_id: Uuid, t: Option<DateTime<Utc>>) -> Self {
        let minted = time_ordered_instant(request_id).map(|from| Probe {
            request_id,
            from,
            until: from + TIME_ORDERED_PROBE,
        });
        let given = t.map(|at| Probe::exact(request_id, at));
        let probes: Vec<Probe> = minted.into_iter().chain(given).collect();
        if !probes.is_empty() {
            return Self::Probes(probes);
        }
        match request_id.get_version_num() {
            RANDOM_VERSION => Self::ByDetail(request_id),
            _ => Self::NeedsTimestamp(request_id),
        }
    }
}

impl Probe {
    fn exact(request_id: Uuid, at: DateTime<Utc>) -> Self {
        Self {
            request_id,
            from: at,
            until: at + EXACT_INSTANT,
        }
    }
}

impl StoredSubject {
    pub fn represents(&self, subject: &Subject) -> bool {
        if self.form != subject.form.code() {
            return false;
        }
        match &subject.name {
            Some(name) => self.name.as_ref() == Some(name),
            None => self.name.is_none() && self.player == subject.player,
        }
    }

    pub fn into_subject(self) -> Option<Subject> {
        let form = Form::from_code(self.form)?;
        let raw = self
            .name
            .clone()
            .or_else(|| self.player.map(|player| player.simple().to_string()))?;
        Some(Subject {
            player: self.player,
            name: self.name,
            raw,
            form,
        })
    }
}

pub async fn first_located(db: &Database, probes: &[Probe]) -> Result<Option<Located>, AdminError> {
    for probe in probes {
        if let Some(located) = locate(db, *probe).await? {
            return Ok(Some(located));
        }
    }
    Ok(None)
}

async fn locate(db: &Database, probe: Probe) -> Result<Option<Located>, AdminError> {
    let (row, detail, subjects) = tokio::try_join!(
        log_row(db, probe),
        detail(db, probe),
        stored_subjects(db, probe),
    )?;
    Ok(row.map(|row| Located {
        row,
        detail,
        subjects,
    }))
}

pub async fn probe_by_detail(db: &Database, request_id: Uuid) -> Result<Option<Probe>, AdminError> {
    let ts: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT ts FROM api_request_detail WHERE request_id = $1 LIMIT 1")
            .bind(request_id)
            .fetch_optional(db.pool())
            .await
            .context("find the request's time through its stored detail")?;
    Ok(ts.map(|at| Probe::exact(request_id, at)))
}

async fn log_row(db: &Database, probe: Probe) -> Result<Option<LogRow>, AdminError> {
    let sql = format!(
        "{} WHERE l.ts >= $1 AND l.ts < $2 \
         AND (l.request_id = $3 OR (l.request_id IS NULL AND md5('arl:' || l.id)::uuid = $3)) \
         LIMIT 1",
        log_row_select()
    );
    sqlx::query_as(&sql)
        .bind(probe.from)
        .bind(probe.until)
        .bind(probe.request_id)
        .fetch_optional(db.pool())
        .await
        .context("read the request")
}

async fn detail(db: &Database, probe: Probe) -> Result<Option<StoredDetail>, AdminError> {
    sqlx::query_as(
        "SELECT ts, headers, body, body_bytes, body_complete, server_error
         FROM api_request_detail
         WHERE ts >= $1 AND ts < $2 AND request_id = $3
         LIMIT 1",
    )
    .bind(probe.from)
    .bind(probe.until)
    .bind(probe.request_id)
    .fetch_optional(db.pool())
    .await
    .context("read the request's stored detail")
}

async fn stored_subjects(db: &Database, probe: Probe) -> Result<Vec<StoredSubject>, AdminError> {
    sqlx::query_as(
        "SELECT form, player, name FROM api_request_subject
         WHERE ts >= $1 AND ts < $2 AND request_id = $3
         ORDER BY form, player, name",
    )
    .bind(probe.from)
    .bind(probe.until)
    .bind(probe.request_id)
    .fetch_all(db.pool())
    .await
    .context("read the request's stored player subjects")
}

fn time_ordered_instant(request_id: Uuid) -> Option<DateTime<Utc>> {
    if request_id.get_version_num() != reqlog::TIME_ORDERED_VERSION {
        return None;
    }
    let (secs, nanos) = request_id.get_timestamp()?.to_unix();
    DateTime::from_timestamp(secs as i64, nanos)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn time_ordered_ids_probe_the_second_they_were_minted_in_first() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 14, 21, 41, 7).unwrap()
            + TimeDelta::microseconds(556_439);
        let id = reqlog::request_id_for(ts);
        let Lookup::Probes(probes) = Lookup::of(id, None) else {
            panic!("a v7 id is probed by its own instant");
        };
        let [minted] = probes.as_slice() else {
            panic!("an id without t has only its minted probe");
        };
        assert!(minted.from <= ts && ts < minted.until);
        assert_eq!(minted.until - minted.from, TIME_ORDERED_PROBE);
        let given = ts - TimeDelta::days(1);
        assert_eq!(
            Lookup::of(id, Some(given)),
            Lookup::Probes(vec![*minted, Probe::exact(id, given)])
        );
    }

    #[test]
    fn other_ids_use_the_given_instant_exactly() {
        let at = Utc.with_ymd_and_hms(2026, 10, 9, 17, 22, 41).unwrap();
        let random = Uuid::parse_str("1c4b2a90-5d6e-4f70-8a91-b2c3d4e5f607").unwrap();
        assert_eq!(
            Lookup::of(random, Some(at)),
            Lookup::Probes(vec![Probe {
                request_id: random,
                from: at,
                until: at + EXACT_INSTANT
            }])
        );
    }

    #[test]
    fn without_an_instant_random_ids_go_through_detail_and_derived_ids_need_one() {
        let random = Uuid::parse_str("1c4b2a90-5d6e-4f70-8a91-b2c3d4e5f607").unwrap();
        let derived = Uuid::parse_str("1c4b2a90-5d6e-1f70-8a91-b2c3d4e5f607").unwrap();
        assert_eq!(Lookup::of(random, None), Lookup::ByDetail(random));
        assert_eq!(Lookup::of(derived, None), Lookup::NeedsTimestamp(derived));
    }

    #[test]
    fn stored_subjects_stand_for_the_extracted_subject_of_their_form() {
        let player = Uuid::parse_str("94589c87999b4a7ab3add2ae50711e64").unwrap();
        let by_name = Subject {
            player: None,
            name: Some("ojugg".into()),
            raw: "oJugg".into(),
            form: Form::Name,
        };
        let resolved = StoredSubject {
            form: Form::Name.code(),
            player: Some(player),
            name: Some("ojugg".into()),
        };
        assert!(resolved.represents(&by_name));
        assert!(
            !StoredSubject {
                form: Form::Path.code(),
                ..resolved.clone()
            }
            .represents(&by_name)
        );
        assert_eq!(
            resolved.into_subject().map(|subject| subject.raw),
            Some("ojugg".into())
        );
    }
}
