use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use database::Database;
use moka::future::Cache;
use reqlog::{Form, KeyKind, Service};
use serde::Serialize;
use sqlx::{FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

use super::forms::{FormSelection, PlayerForm, PlayerWindow};
use super::requester::{Accounts, RequesterKey};
use super::resolved::ResolvedNames;
use crate::coverage::Coverage;
use crate::error::{AdminError, Context};
use crate::query::{PlayerInput, PlayerKeys, PlayerSearch};
use crate::refs::EntityRef;
use crate::serde_ts;

pub const LOOKUPS_FRESH_FOR: TimeDelta = TimeDelta::seconds(15);
pub const LOOKUPS_KEPT_FOR: Duration = Duration::from_secs(5 * 60);
pub const LOOKUPS_CAPACITY: u64 = 1_000;
pub const REFRESH_CLAIM_TTL: Duration = Duration::from_secs(30);

const TOO_MANY_REQUESTS: i16 = 429;
const FIRST_ERROR_STATUS: i16 = 400;
const DAY_AGE: i16 = 0;
const WEEK_AGE: i16 = 1;
const FORTNIGHT_AGE: i16 = 2;

type Sql = QueryBuilder<'static, Postgres>;

#[derive(Clone)]
pub struct PlayerLookups {
    tallies: Cache<Uuid, Arc<LookupTally>>,
    refreshing: Cache<Uuid, ()>,
    db: Arc<Database>,
    players: PlayerSearch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LookupTally {
    pub computed_at: DateTime<Utc>,
    pub from: DateTime<Utc>,
    pub clamped_from: Option<DateTime<Utc>>,
    pub requesters: Vec<RequesterTally>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequesterTally {
    pub key: RequesterKey,
    pub accounts: Accounts,
    pub cells: Vec<TallyCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TallyCell {
    pub form: PlayerForm,
    pub age: PlayerWindow,
    pub requests: i64,
    pub rate_limited: i64,
    pub errors: i64,
    pub first: DateTime<Utc>,
    pub last: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerRequester {
    pub key: RequesterKey,
    pub requester: EntityRef,
    pub actor: Option<EntityRef>,
    pub key_kind: KeyKind,
    pub service: Service,
    pub key_prefix: Option<String>,
    pub requests: i64,
    pub rate_limited: i64,
    pub errors: i64,
    #[serde(serialize_with = "serde_ts::micros")]
    pub first: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub last: DateTime<Utc>,
    pub forms: BTreeMap<PlayerForm, i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RequestTotals {
    pub last_24h: i64,
    pub last_14d: i64,
    pub requesters_14d: i64,
}

#[derive(FromRow)]
struct TallyRow {
    key_kind: i16,
    service: i16,
    member_id: Option<i64>,
    actor_id: Option<i64>,
    key_prefix: Option<String>,
    form: i16,
    former: bool,
    age: i16,
    requests: i64,
    rate_limited: i64,
    errors: i64,
    first: DateTime<Utc>,
    last: DateTime<Utc>,
    member_discord_id: Option<i64>,
    actor_member_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    age: PlayerWindow,
}

#[derive(Debug, Clone, Copy)]
struct TallyWindow {
    from: DateTime<Utc>,
    now: DateTime<Utc>,
}

struct TallyQuery {
    sql: Sql,
    parts: usize,
}

struct HeldColumns {
    names: Vec<String>,
    froms: Vec<Option<DateTime<Utc>>>,
    tos: Vec<Option<DateTime<Utc>>>,
}

impl PlayerLookups {
    pub fn new(db: Arc<Database>, players: PlayerSearch) -> Self {
        Self {
            tallies: Cache::builder()
                .max_capacity(LOOKUPS_CAPACITY)
                .time_to_live(LOOKUPS_KEPT_FOR)
                .build(),
            refreshing: Cache::builder()
                .max_capacity(LOOKUPS_CAPACITY)
                .time_to_live(REFRESH_CLAIM_TTL)
                .build(),
            db,
            players,
        }
    }

    pub async fn tally(
        &self,
        player: Uuid,
        coverage: &Coverage,
    ) -> Result<Arc<LookupTally>, AdminError> {
        let tally = self
            .tallies
            .try_get_with(player, self.load(player, coverage))
            .await
            .map_err(Arc::unwrap_or_clone)?;
        if tally.is_stale(Utc::now()) {
            self.refresh_in_background(player, coverage.clone()).await;
        }
        Ok(tally)
    }

    async fn refresh_in_background(&self, player: Uuid, coverage: Coverage) {
        let claimed = self.refreshing.entry(player).or_insert(()).await.is_fresh();
        if !claimed {
            return;
        }
        let lookups = self.clone();
        tokio::spawn(async move { lookups.refresh(player, &coverage).await });
    }

    async fn refresh(&self, player: Uuid, coverage: &Coverage) {
        match self.load(player, coverage).await {
            Ok(tally) => self.tallies.insert(player, tally).await,
            Err(error) => {
                tracing::warn!(%player, %error, "could not refresh the player's lookup tally")
            }
        }
        self.refreshing.invalidate(&player).await;
    }

    async fn load(
        &self,
        player: Uuid,
        coverage: &Coverage,
    ) -> Result<Arc<LookupTally>, AdminError> {
        let keys = self.players.keys(&PlayerInput::Uuid(player)).await?;
        let tally = LookupTally::read(&self.db, player, &keys, coverage, Utc::now()).await?;
        Ok(Arc::new(tally))
    }
}

impl LookupTally {
    async fn read(
        db: &Database,
        player: Uuid,
        keys: &PlayerKeys,
        coverage: &Coverage,
        computed_at: DateTime<Utc>,
    ) -> Result<Self, AdminError> {
        let window_start = computed_at - PlayerWindow::widest().length();
        let floor = coverage.searchable_subjects_from();
        let from = floor.map_or(window_start, |floor| floor.max(window_start));
        let rows = read_tally(db, player, keys, from, computed_at).await?;
        Ok(Self {
            computed_at,
            from,
            clamped_from: floor.filter(|floor| *floor > window_start),
            requesters: group_requesters(rows),
        })
    }

    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        now - self.computed_at >= LOOKUPS_FRESH_FOR
    }

    pub fn window_start(&self, window: PlayerWindow) -> DateTime<Utc> {
        self.from.max(self.computed_at - window.length())
    }

    pub fn clamped_for(&self, window: PlayerWindow) -> Option<DateTime<Utc>> {
        self.clamped_from
            .filter(|clamp| *clamp > self.computed_at - window.length())
    }

    pub fn requesters(&self, window: PlayerWindow, forms: &FormSelection) -> Vec<PlayerRequester> {
        let mut groups: Vec<PlayerRequester> = self
            .requesters
            .iter()
            .filter_map(|tally| tally.summarize(window, forms))
            .collect();
        groups.sort_by(|a, b| b.requests.cmp(&a.requests).then(b.last.cmp(&a.last)));
        groups
    }

    pub fn form_totals(&self, window: PlayerWindow) -> BTreeMap<PlayerForm, i64> {
        let mut totals = BTreeMap::new();
        for cell in self.cells_within(window) {
            *totals.entry(cell.form).or_default() += cell.requests;
        }
        totals
    }

    pub fn totals(&self) -> RequestTotals {
        let standard = FormSelection::standard();
        let count = |window: PlayerWindow| -> i64 {
            self.cells_within(window)
                .filter(|cell| standard.contains(cell.form))
                .map(|cell| cell.requests)
                .sum()
        };
        RequestTotals {
            last_24h: count(PlayerWindow::Day),
            last_14d: count(PlayerWindow::Fortnight),
            requesters_14d: self.requesters(PlayerWindow::Fortnight, &standard).len() as i64,
        }
    }

    pub fn account_ids(&self) -> Vec<i64> {
        self.requesters
            .iter()
            .flat_map(|tally| [tally.accounts.member_discord_id, tally.key.actor_id])
            .flatten()
            .collect()
    }

    fn cells_within(&self, window: PlayerWindow) -> impl Iterator<Item = &TallyCell> {
        self.requesters
            .iter()
            .flat_map(|tally| tally.cells.iter())
            .filter(move |cell| window.covers(cell.age))
    }
}

impl RequesterTally {
    fn summarize(&self, window: PlayerWindow, forms: &FormSelection) -> Option<PlayerRequester> {
        let within: Vec<&TallyCell> = self
            .cells
            .iter()
            .filter(|cell| window.covers(cell.age))
            .collect();
        let selected: Vec<&&TallyCell> = within
            .iter()
            .filter(|cell| forms.contains(cell.form))
            .collect();
        let first = selected.iter().map(|cell| cell.first).min()?;
        let last = selected.iter().map(|cell| cell.last).max()?;
        let mut by_form = BTreeMap::new();
        for cell in &within {
            *by_form.entry(cell.form).or_default() += cell.requests;
        }
        Some(PlayerRequester {
            requester: self.key.requester(self.accounts),
            actor: self.key.actor(self.accounts),
            key_kind: self.key.key_kind(),
            service: self.key.service(),
            key_prefix: self.key.key_prefix.clone(),
            requests: selected.iter().map(|cell| cell.requests).sum(),
            rate_limited: selected.iter().map(|cell| cell.rate_limited).sum(),
            errors: selected.iter().map(|cell| cell.errors).sum(),
            first,
            last,
            forms: by_form,
            key: self.key.clone(),
        })
    }
}

impl HeldColumns {
    fn of(keys: &PlayerKeys) -> Self {
        Self {
            names: keys.names.iter().map(|held| held.name.clone()).collect(),
            froms: keys.names.iter().map(|held| held.from).collect(),
            tos: keys.names.iter().map(|held| held.to).collect(),
        }
    }
}

async fn read_tally(
    db: &Database,
    player: Uuid,
    keys: &PlayerKeys,
    from: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Vec<TallyRow>, AdminError> {
    let (older, recent): (Vec<Segment>, Vec<Segment>) = Segment::between(from, now)
        .into_iter()
        .partition(|segment| segment.age == PlayerWindow::Fortnight);
    let resolved = ResolvedNames::of(player, keys);
    let window = TallyWindow { from, now };
    let mut older_query = TallyQuery::new();
    for segment in &older {
        push_uuid_segment(&resolved, segment, older_query.part());
    }
    let mut recent_query = TallyQuery::new();
    for segment in &recent {
        push_uuid_segment(&resolved, segment, recent_query.part());
    }
    if !keys.names.is_empty() {
        push_held_names(keys, window, recent_query.part());
    }
    let mut named_query = TallyQuery::new();
    if let Some(current) = resolved.current() {
        push_resolved_names(&resolved, current, window, named_query.part());
    }
    let (older_rows, recent_rows, named_rows) = tokio::try_join!(
        older_query.fetch(db),
        recent_query.fetch(db),
        named_query.fetch(db),
    )?;
    Ok(older_rows
        .into_iter()
        .chain(recent_rows)
        .chain(named_rows)
        .collect())
}

impl TallyQuery {
    fn new() -> Self {
        Self {
            sql: Sql::new(
                "SELECT g.*, m.discord_id AS member_discord_id, am.id AS actor_member_id FROM (",
            ),
            parts: 0,
        }
    }

    fn part(&mut self) -> &mut Sql {
        if self.parts > 0 {
            self.sql.push(" UNION ALL ");
        }
        self.parts += 1;
        &mut self.sql
    }

    async fn fetch(mut self, db: &Database) -> Result<Vec<TallyRow>, AdminError> {
        if self.parts == 0 {
            return Ok(Vec::new());
        }
        self.sql.push(
            ") g LEFT JOIN members m ON m.id = g.member_id \
             LEFT JOIN members am ON am.discord_id = g.actor_id",
        );
        self.sql
            .build_query_as()
            .persistent(false)
            .fetch_all(db.pool())
            .await
            .context("count the player's lookups per requester")
    }
}

fn push_uuid_segment(resolved: &ResolvedNames, segment: &Segment, sql: &mut Sql) {
    sql.push("SELECT s.key_kind, s.service, s.member_id, s.actor_id, s.key_prefix, s.form, ")
        .push_bind(age_code(segment.age))
        .push("::int2 AS age, false AS former, ");
    push_counts(sql);
    sql.push(" FROM api_request_subject s WHERE s.player = ")
        .push_bind(resolved.player())
        .push(" AND s.ts >= ")
        .push_bind(segment.from)
        .push(" AND s.ts < ")
        .push_bind(segment.to);
    if resolved.current().is_some() {
        sql.push(format!(" AND s.form <> {}", Form::Name.code()));
    }
    sql.push(" GROUP BY 1, 2, 3, 4, 5, 6");
}

fn push_resolved_names(
    resolved: &ResolvedNames,
    current: &str,
    window: TallyWindow,
    sql: &mut Sql,
) {
    let name = Form::Name.code();
    sql.push("SELECT s.key_kind, s.service, s.member_id, s.actor_id, s.key_prefix, s.form, ");
    push_age(window.now, sql);
    sql.push(" AS age, named.former, ");
    push_counts(sql);
    sql.push(" FROM api_request_subject s CROSS JOIN LATERAL (SELECT n.name IS DISTINCT FROM ")
        .push_bind(current.to_string())
        .push("::text AS former FROM api_request_subject n WHERE n.player = ")
        .push_bind(resolved.player())
        .push(format!(
            " AND n.ts = s.ts AND n.form = {name} LIMIT 1) named WHERE s.player = "
        ))
        .push_bind(resolved.player())
        .push(" AND s.ts >= ")
        .push_bind(window.from)
        .push(" AND s.ts < ")
        .push_bind(window.now)
        .push(format!(
            " AND s.form = {name} GROUP BY 1, 2, 3, 4, 5, 6, 7, 8"
        ));
}

fn push_held_names(keys: &PlayerKeys, window: TallyWindow, sql: &mut Sql) {
    let held = HeldColumns::of(keys);
    sql.push("SELECT s.key_kind, s.service, s.member_id, s.actor_id, s.key_prefix, s.form, ");
    push_age(window.now, sql);
    sql.push(" AS age, v.held_to IS NOT NULL AS former, ");
    push_counts(sql);
    sql.push(" FROM unnest(")
        .push_bind(held.names)
        .push("::text[], ")
        .push_bind(held.froms)
        .push("::timestamptz[], ")
        .push_bind(held.tos)
        .push(
            "::timestamptz[]) AS v(name, held_from, held_to) \
             JOIN api_request_subject s ON s.player IS NULL AND s.name = v.name \
             AND s.ts >= coalesce(v.held_from, '-infinity') AND s.ts < coalesce(v.held_to, 'infinity') \
             AND s.ts >= ",
        )
        .push_bind(window.from)
        .push(" AND s.ts < ")
        .push_bind(window.now)
        .push(" GROUP BY 1, 2, 3, 4, 5, 6, 7, 8");
}

fn push_age(now: DateTime<Utc>, sql: &mut Sql) {
    sql.push("CASE WHEN s.ts >= ")
        .push_bind(now - PlayerWindow::Day.length())
        .push(format!(" THEN {DAY_AGE} WHEN s.ts >= "))
        .push_bind(now - PlayerWindow::Week.length())
        .push(format!(" THEN {WEEK_AGE} ELSE {FORTNIGHT_AGE} END::int2"));
}

fn push_counts(sql: &mut Sql) {
    sql.push("count(*) AS requests, count(*) FILTER (WHERE s.status = ")
        .push_bind(TOO_MANY_REQUESTS)
        .push(") AS rate_limited, count(*) FILTER (WHERE s.status >= ")
        .push_bind(FIRST_ERROR_STATUS)
        .push(" AND s.status <> ")
        .push_bind(TOO_MANY_REQUESTS)
        .push(") AS errors, min(s.ts) AS first, max(s.ts) AS last");
}

impl Segment {
    fn between(from: DateTime<Utc>, now: DateTime<Utc>) -> Vec<Self> {
        let day_start = now - PlayerWindow::Day.length();
        let week_start = now - PlayerWindow::Week.length();
        let mut cuts: Vec<DateTime<Utc>> = [from, week_start, day_start, now]
            .into_iter()
            .filter(|cut| *cut >= from && *cut <= now)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        cuts.windows(2)
            .map(|pair| Self {
                from: pair[0],
                to: pair[1],
                age: match pair[0] {
                    start if start >= day_start => PlayerWindow::Day,
                    start if start >= week_start => PlayerWindow::Week,
                    _ => PlayerWindow::Fortnight,
                },
            })
            .collect()
    }
}

fn age_code(age: PlayerWindow) -> i16 {
    match age {
        PlayerWindow::Day => DAY_AGE,
        PlayerWindow::Week => WEEK_AGE,
        PlayerWindow::Fortnight => FORTNIGHT_AGE,
    }
}

fn group_requesters(rows: Vec<TallyRow>) -> Vec<RequesterTally> {
    let mut grouped: BTreeMap<RequesterKey, RequesterTally> = BTreeMap::new();
    for row in rows {
        let Some(cell) = row.cell() else {
            continue;
        };
        let key = row.key();
        grouped
            .entry(key.clone())
            .or_insert_with(|| RequesterTally {
                key,
                accounts: Accounts {
                    member_discord_id: row.member_discord_id,
                    actor_member_id: row.actor_member_id,
                },
                cells: Vec::new(),
            })
            .cells
            .push(cell);
    }
    grouped.into_values().collect()
}

impl TallyRow {
    fn key(&self) -> RequesterKey {
        RequesterKey {
            key_kind: self.key_kind,
            service: self.service,
            member_id: self.member_id,
            actor_id: self.actor_id,
            key_prefix: self.key_prefix.clone(),
        }
    }

    fn cell(&self) -> Option<TallyCell> {
        let Some(form) = PlayerForm::from_stored(self.form, self.former) else {
            tracing::warn!(code = self.form, "subject row has an unknown form");
            return None;
        };
        Some(TallyCell {
            form,
            age: age_of(self.age),
            requests: self.requests,
            rate_limited: self.rate_limited,
            errors: self.errors,
            first: self.first,
            last: self.last,
        })
    }
}

fn age_of(code: i16) -> PlayerWindow {
    match code {
        DAY_AGE => PlayerWindow::Day,
        WEEK_AGE => PlayerWindow::Week,
        _ => PlayerWindow::Fortnight,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use coral_redis::RedisPool;
    use reqlog::Form;

    use super::*;
    use crate::players::fixtures::{self, Lookup, lookup};
    use crate::query::HeldName;

    const OLD_NAME: &str = "tally_old";
    const CURRENT_NAME: &str = "tally_now";
    const REFRESH_WAIT: Duration = Duration::from_secs(10);
    const REFRESH_POLL: Duration = Duration::from_millis(20);

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, 12, 0, 0).unwrap()
    }

    fn cell(form: PlayerForm, age: PlayerWindow, requests: i64, day: u32) -> TallyCell {
        TallyCell {
            form,
            age,
            requests,
            rate_limited: 1,
            errors: 0,
            first: at(day),
            last: at(day),
        }
    }

    fn member(id: i64, cells: Vec<TallyCell>) -> RequesterTally {
        RequesterTally {
            key: RequesterKey {
                key_kind: KeyKind::Personal.code(),
                service: Service::Unknown.code(),
                member_id: Some(id),
                actor_id: None,
                key_prefix: None,
            },
            accounts: Accounts {
                member_discord_id: Some(300_000_000_000_000_000 + id),
                actor_member_id: None,
            },
            cells,
        }
    }

    fn tally() -> LookupTally {
        LookupTally {
            computed_at: at(15),
            from: at(1),
            clamped_from: None,
            requesters: vec![
                member(
                    1,
                    vec![
                        cell(PlayerForm::Undashed, PlayerWindow::Day, 5, 15),
                        cell(PlayerForm::Former, PlayerWindow::Fortnight, 7, 3),
                    ],
                ),
                member(
                    2,
                    vec![cell(PlayerForm::Display, PlayerWindow::Week, 9, 10)],
                ),
            ],
        }
    }

    #[test]
    fn requesters_sum_only_the_selected_forms_inside_the_window() {
        let tally = tally();
        let standard = FormSelection::standard();
        let fortnight = tally.requesters(PlayerWindow::Fortnight, &standard);
        assert_eq!(fortnight.len(), 1);
        assert_eq!(fortnight[0].requests, 12);
        assert_eq!(fortnight[0].rate_limited, 2);
        assert_eq!(fortnight[0].first, at(3));
        assert_eq!(fortnight[0].last, at(15));
        let day = tally.requesters(PlayerWindow::Day, &standard);
        assert_eq!(day[0].requests, 5);
        assert_eq!(day[0].forms, BTreeMap::from([(PlayerForm::Undashed, 5)]));
    }

    #[test]
    fn display_only_requesters_appear_once_display_is_selected() {
        let tally = tally();
        let with_display = FormSelection::parse(None, true).unwrap();
        let groups = tally.requesters(PlayerWindow::Week, &with_display);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].requests, 9);
    }

    #[test]
    fn form_totals_count_every_form_and_totals_skip_display() {
        let tally = tally();
        assert_eq!(
            tally.form_totals(PlayerWindow::Fortnight),
            BTreeMap::from([
                (PlayerForm::Undashed, 5),
                (PlayerForm::Former, 7),
                (PlayerForm::Display, 9),
            ])
        );
        assert_eq!(
            tally.totals(),
            RequestTotals {
                last_24h: 5,
                last_14d: 12,
                requesters_14d: 1
            }
        );
    }

    #[test]
    fn segments_cut_at_each_age() {
        let segments = Segment::between(at(1), at(15));
        let shape: Vec<(DateTime<Utc>, DateTime<Utc>, PlayerWindow)> = segments
            .iter()
            .map(|segment| (segment.from, segment.to, segment.age))
            .collect();
        assert_eq!(
            shape,
            vec![
                (at(1), at(8), PlayerWindow::Fortnight),
                (at(8), at(14), PlayerWindow::Week),
                (at(14), at(15), PlayerWindow::Day),
            ]
        );
        assert_eq!(Segment::between(at(10), at(15))[0].age, PlayerWindow::Week);
        assert!(Segment::between(at(15), at(15)).is_empty());
    }

    #[test]
    fn windows_start_at_the_coverage_floor_when_it_is_later() {
        let tally = LookupTally {
            from: at(10),
            clamped_from: Some(at(10)),
            ..tally()
        };
        assert_eq!(tally.window_start(PlayerWindow::Fortnight), at(10));
        assert_eq!(tally.clamped_for(PlayerWindow::Fortnight), Some(at(10)));
        assert_eq!(tally.window_start(PlayerWindow::Day), at(14));
        assert_eq!(tally.clamped_for(PlayerWindow::Day), None);
    }

    async fn lookups() -> Option<PlayerLookups> {
        let db = fixtures::database().await?;
        let redis = RedisPool::connect(&std::env::var("REDIS_URL").ok()?)
            .await
            .ok()?;
        Some(PlayerLookups::new(db.clone(), PlayerSearch::new(db, redis)))
    }

    fn counted_at(computed_at: DateTime<Utc>) -> Arc<LookupTally> {
        Arc::new(LookupTally {
            computed_at,
            ..tally()
        })
    }

    async fn replacement_of(
        lookups: &PlayerLookups,
        player: Uuid,
        stale: &Arc<LookupTally>,
    ) -> Arc<LookupTally> {
        loop {
            match lookups.tallies.get(&player).await {
                Some(current) if !Arc::ptr_eq(&current, stale) => return current,
                _ => tokio::time::sleep(REFRESH_POLL).await,
            }
        }
    }

    #[tokio::test]
    async fn a_fresh_tally_is_served_from_the_cache_without_a_refresh() {
        let Some(lookups) = lookups().await else {
            return;
        };
        let player = Uuid::parse_str("73000016000040008000000000000001").unwrap();
        let cached = counted_at(Utc::now());
        lookups.tallies.insert(player, cached.clone()).await;

        let served = lookups.tally(player, &Coverage::default()).await.unwrap();

        assert!(Arc::ptr_eq(&served, &cached));
        assert!(!lookups.refreshing.contains_key(&player));
    }

    #[tokio::test]
    async fn a_stale_tally_is_served_at_once_and_recounted_in_the_background() {
        let Some(lookups) = lookups().await else {
            return;
        };
        let player = Uuid::parse_str("73000017000040008000000000000001").unwrap();
        let stale = counted_at(Utc::now() - LOOKUPS_FRESH_FOR - TimeDelta::seconds(1));
        lookups.tallies.insert(player, stale.clone()).await;

        let served = lookups.tally(player, &Coverage::default()).await.unwrap();

        assert!(Arc::ptr_eq(&served, &stale));
        let recounted =
            tokio::time::timeout(REFRESH_WAIT, replacement_of(&lookups, player, &stale))
                .await
                .expect("the stale tally is recounted");
        assert!(!recounted.is_stale(Utc::now()));
        assert!(recounted.requesters.is_empty());
    }

    fn by_member(lookup: Lookup, id: i64) -> Lookup {
        Lookup {
            member_id: Some(id),
            key_kind: KeyKind::Personal,
            key_prefix: None,
            ..lookup
        }
    }

    #[tokio::test]
    async fn the_tally_counts_forms_ages_and_requesters_from_the_subject_rows() {
        let Some(db) = fixtures::database().await else {
            return;
        };
        let player = Uuid::parse_str("7300000a000040008000000000000001").unwrap();
        fixtures::forget_player(&db, player, &[OLD_NAME, CURRENT_NAME]).await;
        let now = fixtures::end_of_days();
        let hours = |count: i64| now - TimeDelta::hours(count);
        fixtures::record(
            &db,
            &[
                by_member(lookup(hours(1), Form::Undashed, Some(player)), 25),
                Lookup {
                    status: TOO_MANY_REQUESTS,
                    ..by_member(lookup(hours(2), Form::Undashed, Some(player)), 25)
                },
                Lookup {
                    name: Some(CURRENT_NAME),
                    ..by_member(lookup(hours(30), Form::Name, Some(player)), 25)
                },
                Lookup {
                    name: Some(CURRENT_NAME),
                    ..by_member(lookup(hours(80), Form::Name, Some(player)), 25)
                },
                Lookup {
                    name: Some(OLD_NAME),
                    ..by_member(lookup(hours(100), Form::Name, Some(player)), 26)
                },
                Lookup {
                    name: Some(OLD_NAME),
                    ..lookup(hours(120), Form::Name, None)
                },
                Lookup {
                    name: Some(OLD_NAME),
                    ..lookup(hours(10), Form::Name, None)
                },
                Lookup {
                    name: Some(CURRENT_NAME),
                    ..lookup(hours(3), Form::Display, None)
                },
                Lookup {
                    key_kind: KeyKind::Internal,
                    service: Service::Bot,
                    actor_id: Some(111),
                    key_prefix: Some("intkey00"),
                    ..lookup(hours(200), Form::Body, Some(player))
                },
            ],
        )
        .await;
        let keys = PlayerKeys {
            uuids: vec![player],
            names: vec![
                HeldName {
                    name: OLD_NAME.into(),
                    from: Some(hours(250)),
                    to: Some(hours(72)),
                },
                HeldName {
                    name: CURRENT_NAME.into(),
                    from: Some(hours(72)),
                    to: None,
                },
            ],
        };
        let coverage = Coverage {
            subjects_from: Some(fixtures::origin()),
            ..Coverage::default()
        };

        let tally = LookupTally::read(&db, player, &keys, &coverage, now)
            .await
            .unwrap();

        assert_eq!(tally.clamped_from, Some(fixtures::origin()));
        assert_eq!(
            tally.form_totals(PlayerWindow::Fortnight),
            BTreeMap::from([
                (PlayerForm::Undashed, 2),
                (PlayerForm::Name, 2),
                (PlayerForm::Former, 2),
                (PlayerForm::Body, 1),
                (PlayerForm::Display, 1),
            ])
        );
        assert_eq!(
            tally.form_totals(PlayerWindow::Day),
            BTreeMap::from([(PlayerForm::Undashed, 2), (PlayerForm::Display, 1)])
        );
        let groups = tally.requesters(PlayerWindow::Fortnight, &FormSelection::standard());
        assert_eq!(groups.len(), 4);
        let regular = groups
            .iter()
            .find(|group| group.key.member_id == Some(25))
            .unwrap();
        assert_eq!((regular.requests, regular.rate_limited), (4, 1));
        assert_eq!(regular.first, hours(80));
        assert_eq!(regular.last, hours(1));
        let bot = groups
            .iter()
            .find(|group| group.key_kind == KeyKind::Internal)
            .unwrap();
        assert_eq!(
            bot.requester,
            EntityRef::Internal {
                service: Service::Bot
            }
        );
        assert_eq!(
            bot.actor,
            Some(EntityRef::Account {
                discord_id: 111,
                member_id: None
            })
        );
        assert_eq!(
            tally.totals(),
            RequestTotals {
                last_24h: 2,
                last_14d: 7,
                requesters_14d: 4
            }
        );
        fixtures::forget_player(&db, player, &[OLD_NAME, CURRENT_NAME]).await;
    }
}
