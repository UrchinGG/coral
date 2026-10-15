use std::sync::Arc;

use chrono::{DateTime, Utc};
use database::Database;
use reqlog::Form;
use serde::Serialize;
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use super::forms::{FORMER_NAME_CODE, FormSelection, PlayerForm};
use super::params::StreamParams;
use super::requester::RequesterKey;
use super::resolved::ResolvedNames;
use crate::error::{AdminError, Context};
use crate::query::{Cursor, LogRow, PlayerInput, PlayerKeys};
use crate::refs::{Match, Page, RequestRow, Subject};
use crate::requests::{ClientNetworks, labels_for, listed_subjects, request_row};
use crate::state::AppState;

pub const STREAM_PAGE_ROWS: usize = 100;

const REQUEST_KEY: &str = "coalesce(l.request_id, md5('arl:' || l.id)::uuid)";
const LEGACY_ROW: &str = "(l.request_id IS NULL OR uuid_extract_version(l.request_id) <> 7)";
const PROBED_COLUMNS: &str = "s.ts, s.request_id, s.name, s.form";
const LOG_COLUMNS: &str = "l.ts, l.method, l.route, l.path, l.query, l.status, l.outcome_status, \
     l.cause, l.error, l.latency_ms, l.key_kind, l.key_prefix, l.member_id, l.actor_id, l.ip, \
     l.country, l.user_agent, l.subjects";

#[derive(Debug, Clone, Serialize)]
pub struct PlayerRequestRow {
    #[serde(flatten)]
    pub request: RequestRow,
    pub matched_form: PlayerForm,
}

#[derive(Debug, Clone, FromRow)]
struct StreamRow {
    #[sqlx(flatten)]
    log: LogRow,
    category: i16,
}

#[derive(Debug, Clone)]
struct StreamPlan {
    player: Uuid,
    keys: Arc<PlayerKeys>,
    from: DateTime<Utc>,
    forms: FormSelection,
    requester: Option<RequesterKey>,
    cursor: Option<Cursor>,
}

#[derive(Debug, Clone)]
struct StreamPage {
    rows: Vec<StreamRow>,
    next_cursor: Option<Cursor>,
}

#[derive(Debug, Clone, Copy)]
enum Arm<'a> {
    Uuid(&'a ResolvedNames),
    Name,
}

type Sql = QueryBuilder<'static, Postgres>;

pub async fn read_stream(
    state: &AppState,
    player: Uuid,
    params: StreamParams,
) -> Result<Page<PlayerRequestRow>, AdminError> {
    let coverage = state.coverage.current();
    let window_start = Utc::now() - params.window.length();
    let floor = coverage.searchable_subjects_from();
    let plan = StreamPlan {
        player,
        keys: state.players.keys(&PlayerInput::Uuid(player)).await?,
        from: floor.map_or(window_start, |floor| floor.max(window_start)),
        forms: params.forms()?,
        requester: params.requester()?,
        cursor: params.cursor,
    };
    let page = plan.fetch(&state.db).await?;
    let subjects: Vec<Vec<Subject>> = page
        .rows
        .iter()
        .map(|row| listed_subjects(&row.log))
        .collect();
    let logs = || page.rows.iter().map(|row| &row.log);
    let wanted = labels_for(logs(), subjects.iter().flatten());
    let (networks, labels) = tokio::try_join!(
        ClientNetworks::read(&state.db, logs(), &coverage),
        state.names.cached(&wanted),
    )?;
    let items = page
        .rows
        .iter()
        .zip(subjects)
        .map(|(row, subjects)| player_request_row(row, subjects, &networks))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page {
        next_cursor: page.next_cursor,
        scanned_to: None,
        clamped_from: floor.filter(|floor| *floor > window_start),
        items,
        labels,
    })
}

impl StreamPlan {
    async fn fetch(&self, db: &Database) -> Result<StreamPage, AdminError> {
        let mut rows: Vec<StreamRow> = stream_sql(self)
            .build_query_as()
            .persistent(false)
            .fetch_all(db.pool())
            .await
            .context("read the requests that name the player")?;
        let full = rows.len() > STREAM_PAGE_ROWS;
        rows.truncate(STREAM_PAGE_ROWS);
        let next_cursor = full
            .then(|| rows.last().map(|row| row.log.cursor()))
            .flatten();
        Ok(StreamPage { rows, next_cursor })
    }
}

fn player_request_row(
    row: &StreamRow,
    subjects: Vec<Subject>,
    networks: &ClientNetworks,
) -> Result<PlayerRequestRow, AdminError> {
    let matched_form = PlayerForm::from_code(row.category).ok_or_else(|| {
        AdminError::Internal(format!(
            "a subject of request {} has the unknown form {}",
            row.log.request_key, row.category
        ))
    })?;
    Ok(PlayerRequestRow {
        request: request_row(&row.log, subjects, networks.of(&row.log)),
        matched_form,
    })
}

fn stream_sql(plan: &StreamPlan) -> Sql {
    let fetch = STREAM_PAGE_ROWS as i64 + 1;
    let mut sql = Sql::new(format!(
        "SELECT {LOG_COLUMNS}, {REQUEST_KEY} AS request_key, {LEGACY_ROW} AS legacy, \
         hits.matched, hits.category, m.discord_id AS member_discord_id, am.id AS actor_member_id \
         FROM (SELECT DISTINCT ON (h.ts, h.request_id) h.ts, h.request_id, h.matched, h.category FROM ("
    ));
    push_uuid_arm(plan, fetch, &mut sql);
    if !plan.keys.names.is_empty() {
        sql.push(" UNION ALL ");
        push_name_arm(plan, fetch, &mut sql);
    }
    sql.push(") h ORDER BY h.ts DESC, h.request_id DESC, h.rank LIMIT ")
        .push_bind(fetch);
    sql.push(format!(
        ") hits JOIN api_request_log l ON l.ts = hits.ts AND {REQUEST_KEY} = hits.request_id \
         LEFT JOIN members m ON m.id = l.member_id \
         LEFT JOIN members am ON am.discord_id = l.actor_id \
         ORDER BY l.ts DESC, request_key DESC"
    ));
    sql
}

fn push_uuid_arm(plan: &StreamPlan, fetch: i64, sql: &mut Sql) {
    let resolved = ResolvedNames::of(plan.player, &plan.keys);
    sql.push("SELECT s.ts, s.request_id, ")
        .push(match_columns(
            "s.name IS NOT NULL",
            Match::ResolvedName,
            Match::Uuid,
        ))
        .push(", ");
    resolved.push_category("s", sql);
    sql.push(
        " AS category FROM (SELECT DISTINCT p.ts FROM api_request_subject p WHERE p.player = ",
    )
    .push_bind(plan.player);
    push_subject_filters(plan, Arm::Uuid(&resolved), "p", sql);
    sql.push(" ORDER BY p.ts DESC LIMIT ")
        .push_bind(fetch + 1)
        .push(format!(
            ") picked CROSS JOIN LATERAL (SELECT {PROBED_COLUMNS} FROM api_request_subject s \
             WHERE s.player = "
        ))
        .push_bind(plan.player)
        .push(" AND s.ts = picked.ts");
    push_subject_filters(plan, Arm::Uuid(&resolved), "s", sql);
    push_exact_keyset(plan.cursor, sql);
    sql.push(") s");
}

fn push_name_arm(plan: &StreamPlan, fetch: i64, sql: &mut Sql) {
    let display = Form::Display.code();
    let names: Vec<String> = plan
        .keys
        .names
        .iter()
        .map(|held| held.name.clone())
        .collect();
    let froms: Vec<Option<DateTime<Utc>>> = plan.keys.names.iter().map(|held| held.from).collect();
    let tos: Vec<Option<DateTime<Utc>>> = plan.keys.names.iter().map(|held| held.to).collect();
    sql.push("SELECT x.ts, x.request_id, ")
        .push(match_columns(
            &format!("x.form = {display}"),
            Match::DisplayName,
            Match::NameOnly,
        ))
        .push(", x.category FROM unnest(")
        .push_bind(names)
        .push("::text[], ")
        .push_bind(froms)
        .push("::timestamptz[], ")
        .push_bind(tos)
        .push(
            "::timestamptz[]) AS v(name, held_from, held_to) CROSS JOIN LATERAL (\
             SELECT s.ts, s.request_id, s.form, ",
        );
    push_name_category("s", sql);
    sql.push(
        " AS category FROM api_request_subject s \
         WHERE s.player IS NULL AND s.name = v.name \
         AND s.ts >= coalesce(v.held_from, '-infinity') AND s.ts < coalesce(v.held_to, 'infinity')",
    );
    push_subject_filters(plan, Arm::Name, "s", sql);
    push_exact_keyset(plan.cursor, sql);
    sql.push(" ORDER BY s.ts DESC, s.request_id DESC LIMIT ")
        .push_bind(fetch)
        .push(") x");
}

fn push_subject_filters(plan: &StreamPlan, arm: Arm<'_>, alias: &'static str, sql: &mut Sql) {
    sql.push(format!(" AND {alias}.ts >= "))
        .push_bind(plan.from);
    if let Some(cursor) = plan.cursor {
        sql.push(format!(" AND {alias}.ts <= "))
            .push_bind(cursor.ts);
    }
    sql.push(" AND ");
    match arm {
        Arm::Uuid(_) if !plan.forms.splits_names() => {
            sql.push(format!("{alias}.form = ANY("))
                .push_bind(plan.forms.stored_codes())
                .push(")");
        }
        Arm::Uuid(resolved) => {
            resolved.push_category(alias, sql);
            sql.push(" = ANY(").push_bind(plan.forms.codes()).push(")");
        }
        Arm::Name => {
            push_name_category(alias, sql);
            sql.push(" = ANY(").push_bind(plan.forms.codes()).push(")");
        }
    }
    if let Some(requester) = &plan.requester {
        push_requester(alias, requester, sql);
    }
}

fn push_name_category(alias: &str, sql: &mut Sql) {
    let name = Form::Name.code();
    sql.push(format!(
        "(CASE WHEN {alias}.form = {name} AND v.held_to IS NOT NULL THEN {FORMER_NAME_CODE} ELSE {alias}.form END)::int2"
    ));
}

fn push_requester(alias: &str, requester: &RequesterKey, sql: &mut Sql) {
    sql.push(format!(" AND {alias}.key_kind = "))
        .push_bind(requester.key_kind);
    sql.push(format!(" AND {alias}.service = "))
        .push_bind(requester.service);
    sql.push(format!(" AND {alias}.member_id IS NOT DISTINCT FROM "))
        .push_bind(requester.member_id)
        .push("::int8");
    sql.push(format!(" AND {alias}.actor_id IS NOT DISTINCT FROM "))
        .push_bind(requester.actor_id)
        .push("::int8");
    sql.push(format!(" AND {alias}.key_prefix IS NOT DISTINCT FROM "))
        .push_bind(requester.key_prefix.clone())
        .push("::text");
}

fn push_exact_keyset(cursor: Option<Cursor>, sql: &mut Sql) {
    let Some(cursor) = cursor else {
        return;
    };
    sql.push(" AND (s.ts < ")
        .push_bind(cursor.ts)
        .push(" OR (s.ts = ")
        .push_bind(cursor.ts)
        .push(" AND s.request_id < ")
        .push_bind(cursor.request_id)
        .push("))");
}

fn match_columns(matches_when: &str, when: Match, otherwise: Match) -> String {
    format!(
        "CASE WHEN {matches_when} THEN '{}' ELSE '{}' END AS matched, \
         CASE WHEN {matches_when} THEN {} ELSE {} END AS rank",
        when.label(),
        otherwise.label(),
        when.preference(),
        otherwise.preference(),
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;
    use reqlog::KeyKind;

    use super::*;
    use crate::players::fixtures::{self, Lookup, lookup};
    use crate::query::HeldName;

    const BATCH: &str = "/v3/players";

    #[derive(Clone, Copy)]
    struct Case {
        player: &'static str,
        old_name: &'static str,
        current_name: &'static str,
    }

    impl Case {
        fn player(&self) -> Uuid {
            Uuid::parse_str(self.player).unwrap()
        }

        fn names(&self) -> [&'static str; 2] {
            [self.old_name, self.current_name]
        }

        fn plan(
            &self,
            forms: FormSelection,
            requester: Option<RequesterKey>,
            cursor: Option<Cursor>,
        ) -> StreamPlan {
            let renamed = fixtures::end_of_days() - TimeDelta::days(5);
            StreamPlan {
                player: self.player(),
                keys: Arc::new(PlayerKeys {
                    uuids: vec![self.player()],
                    names: vec![
                        HeldName {
                            name: self.old_name.into(),
                            from: Some(fixtures::origin()),
                            to: Some(renamed),
                        },
                        HeldName {
                            name: self.current_name.into(),
                            from: Some(renamed),
                            to: None,
                        },
                    ],
                }),
                from: fixtures::origin(),
                forms,
                requester,
                cursor,
            }
        }
    }

    fn member(lookup: Lookup, id: i64) -> Lookup {
        Lookup {
            member_id: Some(id),
            key_kind: KeyKind::Personal,
            key_prefix: None,
            ..lookup
        }
    }

    async fn seeded(case: Case) -> Option<(Arc<Database>, Vec<Uuid>)> {
        let db = fixtures::database().await?;
        let player = case.player();
        fixtures::forget_player(&db, player, &case.names()).await;
        let now = fixtures::end_of_days();
        let minutes = |count: i64| now - TimeDelta::minutes(count);
        let mut lookups: Vec<Lookup> = (1..=STREAM_PAGE_ROWS as i64 + 3)
            .map(|offset| {
                member(
                    lookup(minutes(offset * 10), Form::Undashed, Some(player)),
                    25,
                )
            })
            .collect();
        lookups.push(Lookup {
            name: Some(case.old_name),
            ..lookup(now - TimeDelta::days(6), Form::Name, None)
        });
        lookups.push(Lookup {
            name: Some(case.old_name),
            ..lookup(now - TimeDelta::days(1), Form::Name, None)
        });
        lookups.push(Lookup {
            method: "POST",
            route: BATCH,
            ..member(
                lookup(now - TimeDelta::days(7), Form::Body, Some(player)),
                26,
            )
        });
        lookups.push(Lookup {
            name: Some(case.current_name),
            ..member(
                lookup(
                    now - TimeDelta::days(6) + TimeDelta::hours(1),
                    Form::Name,
                    Some(player),
                ),
                25,
            )
        });
        let ids = fixtures::record(&db, &lookups).await;
        Some((db, ids))
    }

    #[tokio::test]
    async fn the_stream_pages_newest_first_without_gaps_or_repeats() {
        let case = Case {
            player: "7300000b000040008000000000000001",
            old_name: "stream_pages_old",
            current_name: "stream_pages_now",
        };
        let Some((db, ids)) = seeded(case).await else {
            return;
        };
        let first = case
            .plan(FormSelection::standard(), None, None)
            .fetch(&db)
            .await
            .unwrap();
        assert_eq!(first.rows.len(), STREAM_PAGE_ROWS);
        assert!(
            first
                .rows
                .windows(2)
                .all(|pair| pair[0].log.ts > pair[1].log.ts)
        );
        let cursor = first.next_cursor.expect("a full page continues");
        let second = case
            .plan(FormSelection::standard(), None, Some(cursor))
            .fetch(&db)
            .await
            .unwrap();
        assert_eq!(second.next_cursor, None);
        let mut seen: Vec<Uuid> = first
            .rows
            .iter()
            .chain(&second.rows)
            .map(|row| row.log.request_key)
            .collect();
        seen.sort();
        let mut expected: Vec<Uuid> = ids.clone();
        expected.remove(STREAM_PAGE_ROWS + 4);
        expected.sort();
        assert_eq!(seen, expected);
        fixtures::forget_player(&db, case.player(), &case.names()).await;
    }

    #[tokio::test]
    async fn names_match_only_inside_their_held_window_and_forms_filter_the_stream() {
        let case = Case {
            player: "7300000c000040008000000000000001",
            old_name: "stream_forms_old",
            current_name: "stream_forms_now",
        };
        let Some((db, _)) = seeded(case).await else {
            return;
        };
        let former = FormSelection::parse(Some("former"), false).unwrap();
        let rows = case.plan(former, None, None).fetch(&db).await.unwrap().rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].log.matched(), Some(Match::NameOnly));
        assert_eq!(
            PlayerForm::from_code(rows[0].category),
            Some(PlayerForm::Former)
        );

        let name = FormSelection::parse(Some("name"), false).unwrap();
        let rows = case.plan(name, None, None).fetch(&db).await.unwrap().rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].log.matched(), Some(Match::ResolvedName));
        assert_eq!(
            PlayerForm::from_code(rows[0].category),
            Some(PlayerForm::Name)
        );

        let body = FormSelection::parse(Some("body"), false).unwrap();
        let rows = case.plan(body, None, None).fetch(&db).await.unwrap().rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].log.route.as_deref(), Some(BATCH));
        fixtures::forget_player(&db, case.player(), &case.names()).await;
    }

    #[tokio::test]
    async fn a_requester_filter_keeps_only_that_requester() {
        let case = Case {
            player: "7300000d000040008000000000000001",
            old_name: "stream_requester_old",
            current_name: "stream_requester_now",
        };
        let Some((db, _)) = seeded(case).await else {
            return;
        };
        let batcher = RequesterKey {
            key_kind: KeyKind::Personal.code(),
            service: reqlog::Service::Unknown.code(),
            member_id: Some(26),
            actor_id: None,
            key_prefix: None,
        };
        let rows = case
            .plan(FormSelection::standard(), Some(batcher), None)
            .fetch(&db)
            .await
            .unwrap()
            .rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].log.member_id, Some(26));
        fixtures::forget_player(&db, case.player(), &case.names()).await;
    }
}
