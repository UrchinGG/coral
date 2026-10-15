use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, TimeDelta, Utc};
use database::{Member, ReviewRulesRepository, Strike, StrikeRepository, StrikeStatus, standing};
use reqlog::Cause;
use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;

use super::exact_json::exact_integers;
use super::{FIRST_ERROR_STATUS, parse_discord_id};
use crate::error::{AdminError, Context};
use crate::refs::{EntityRef, Labels};
use crate::routes::ref_labels::label_request;
use crate::serde_ts;
use crate::state::AppState;

const DAY: TimeDelta = TimeDelta::hours(24);
const FORTNIGHT: TimeDelta = TimeDelta::days(14);

const MEMBER_ROW: &str = "SELECT id, discord_id, uuid, api_key, join_date, request_count, access_level,
        key_locked, tagging_disabled, accepted_tags, rejected_tags, accurate_verdicts,
        incorrect_verdicts, bonus_verdicts, vote_granted, tag_granted, config, strikes,
        (SELECT count(*) FROM member_strike s WHERE s.member_id = members.id
           AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at > now())) AS active_strikes,
        created_at, to_jsonb(members) AS raw
     FROM members WHERE discord_id = $1";

#[derive(Serialize)]
pub struct AccountCore {
    account: EntityRef,
    member: Option<MemberView>,
    keys: KeySummary,
    minecraft: Vec<MinecraftLink>,
    former_minecraft: i64,
    standing: Option<StandingView>,
    strikes: Strikes,
    starfish: Option<StarfishSummary>,
    activity: ActivitySummary,
    counts: Counts,
    labels: Labels,
}

#[derive(Serialize)]
struct MemberView {
    id: i64,
    uuid: Option<String>,
    access_level: i16,
    key_locked: bool,
    tagging_disabled: bool,
    #[serde(serialize_with = "serde_ts::micros")]
    join_date: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    created_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    last_request_at: Option<DateTime<Utc>>,
    request_count: i64,
    config: Value,
    is_owner: bool,
    row: Value,
}

#[derive(Serialize)]
struct KeySummary {
    personal: Option<String>,
    personal_id: Option<i64>,
    developer: Option<DevKey>,
    history_count: i64,
}

#[derive(Serialize, FromRow)]
pub struct DevKey {
    pub key_id: Option<i64>,
    pub key: String,
    pub label: String,
    pub permissions: i64,
    pub rate_limit: i32,
    pub request_count: i64,
    pub locked: bool,
    #[serde(serialize_with = "serde_ts::micros")]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct MinecraftLink {
    player: EntityRef,
    relation: String,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    linked_at: Option<DateTime<Utc>>,
    also_primary_of: Vec<EntityRef>,
}

#[derive(Serialize)]
struct StandingView {
    can_vote: bool,
    vote_reason: String,
    can_tag: bool,
    tag_reason: String,
    effective_level: i16,
    strike_count: usize,
    accepted_tags: i64,
    rejected_tags: i64,
    accurate_verdicts: i64,
    incorrect_verdicts: i64,
    bonus_verdicts: i64,
}

#[derive(Default, Serialize)]
struct Strikes {
    active: usize,
    items: Vec<StrikeRow>,
}

#[derive(Serialize)]
pub struct StrikeRow {
    pub id: i64,
    pub reason: String,
    pub struck_by: EntityRef,
    #[serde(serialize_with = "serde_ts::micros")]
    pub issued_at: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "serde_ts::micros_opt")]
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by: Option<EntityRef>,
    pub status: &'static str,
}

#[derive(Serialize, FromRow)]
struct StarfishSummary {
    user_id: i64,
    license_status: String,
    github_username: Option<String>,
    active_session: bool,
}

#[derive(Default, Serialize)]
struct ActivitySummary {
    h24: Totals,
    d14: Totals,
    mentions_24h: i64,
    last_ip: Option<EntityRef>,
    ip_count: i64,
}

#[derive(Default, Serialize)]
pub struct Totals {
    pub requests: i64,
    pub rate_limited_caller: i64,
    pub rate_limited_upstream: i64,
    pub errors: i64,
}

#[derive(Default, Serialize)]
struct Counts {
    ips: i64,
    agents: i64,
    links: i64,
    authored_tags: i64,
    reviewed_tags: i64,
    reviews: i64,
    votes: i64,
    admin_actions: i64,
}

#[derive(FromRow)]
struct MemberRow {
    #[sqlx(flatten)]
    member: Member,
    created_at: DateTime<Utc>,
    raw: Value,
}

#[derive(FromRow)]
struct LinkRow {
    uuid: String,
    relation: String,
    linked_at: Option<DateTime<Utc>>,
    shared_discord_ids: Vec<i64>,
    shared_member_ids: Vec<i64>,
}

#[derive(Default, FromRow)]
struct ActivityRow {
    requests_24h: i64,
    caller_24h: i64,
    upstream_24h: i64,
    errors_24h: i64,
    mentions_24h: i64,
    requests_14d: i64,
    caller_14d: i64,
    upstream_14d: i64,
    errors_14d: i64,
}

#[derive(Default, FromRow)]
struct CountRow {
    personal_id: Option<i64>,
    history_count: i64,
    ip_count: i64,
    agent_count: i64,
    last_ip: Option<String>,
    last_country: Option<String>,
    last_proxied: Option<bool>,
    links: i64,
    former_links: i64,
    authored_tags: i64,
    reviewed_tags: i64,
    reviews: i64,
    votes: i64,
    admin_actions: i64,
}

struct Sections {
    developer: Option<DevKey>,
    minecraft: Vec<LinkRow>,
    strikes: Vec<Strike>,
    standing: Option<StandingView>,
    starfish: Option<StarfishSummary>,
    activity: ActivityRow,
    last_request_at: Option<DateTime<Utc>>,
    counts: CountRow,
}

pub async fn account(
    State(state): State<AppState>,
    Path(raw): Path<String>,
) -> Result<Json<AccountCore>, AdminError> {
    let discord_id = parse_discord_id(&raw)?;
    let row: Option<MemberRow> = sqlx::query_as(MEMBER_ROW)
        .bind(discord_id)
        .fetch_optional(state.db.pool())
        .await
        .context("read the member row")?;
    let sections = load_sections(&state, discord_id, row.as_ref()).await?;
    let core = assemble(&state, discord_id, row, sections);
    let labels = state.names.cached(&label_request(mentioned(&core))).await?;
    Ok(Json(AccountCore { labels, ..core }))
}

async fn load_sections(
    state: &AppState,
    discord_id: i64,
    row: Option<&MemberRow>,
) -> Result<Sections, AdminError> {
    let member = row.map(|row| &row.member);
    let member_id = member.map(|member| member.id);
    let now = Utc::now();
    let attribution_from = state.coverage.current().attribution_from;
    let personal_key = member.and_then(|member| member.api_key.as_deref());
    let (developer, minecraft, (strikes, standing), starfish, activity, last_request_at, counts) =
        tokio::try_join!(
            developer(state, member_id),
            minecraft_links(state, member_id),
            strikes_and_standing(state, member),
            starfish_summary(state, discord_id),
            activity_totals(state, member_id, discord_id, now),
            last_request_at(state, member_id, discord_id, attribution_from),
            counts(state, member_id, discord_id, personal_key),
        )?;
    Ok(Sections {
        developer,
        minecraft,
        strikes,
        standing,
        starfish,
        activity,
        last_request_at,
        counts,
    })
}

fn assemble(
    state: &AppState,
    discord_id: i64,
    row: Option<MemberRow>,
    sections: Sections,
) -> AccountCore {
    let member_id = row.as_ref().map(|row| row.member.id);
    let strike_rows: Vec<StrikeRow> = sections.strikes.into_iter().map(strike_row).collect();
    let active = strike_rows
        .iter()
        .filter(|strike| strike.status == "active")
        .count();
    let counts = sections.counts;
    let keys = KeySummary {
        personal: row.as_ref().and_then(|row| row.member.api_key.clone()),
        personal_id: counts.personal_id,
        developer: sections.developer,
        history_count: counts.history_count,
    };
    AccountCore {
        account: EntityRef::Account {
            discord_id,
            member_id,
        },
        member: row.map(|row| MemberView {
            id: row.member.id,
            uuid: row.member.uuid,
            access_level: row.member.access_level,
            key_locked: row.member.key_locked,
            tagging_disabled: row.member.tagging_disabled,
            join_date: row.member.join_date,
            created_at: row.created_at,
            last_request_at: sections.last_request_at,
            request_count: row.member.request_count,
            config: exact_integers(row.member.config),
            is_owner: state.auth.owner_ids.contains(&discord_id),
            row: exact_integers(row.raw),
        }),
        keys,
        minecraft: sections.minecraft.into_iter().map(minecraft_link).collect(),
        former_minecraft: counts.former_links,
        standing: sections.standing,
        strikes: Strikes {
            active,
            items: strike_rows,
        },
        starfish: sections.starfish,
        activity: ActivitySummary {
            h24: Totals {
                requests: sections.activity.requests_24h,
                rate_limited_caller: sections.activity.caller_24h,
                rate_limited_upstream: sections.activity.upstream_24h,
                errors: sections.activity.errors_24h,
            },
            d14: Totals {
                requests: sections.activity.requests_14d,
                rate_limited_caller: sections.activity.caller_14d,
                rate_limited_upstream: sections.activity.upstream_14d,
                errors: sections.activity.errors_14d,
            },
            mentions_24h: sections.activity.mentions_24h,
            last_ip: counts.last_ip.map(|ip| EntityRef::Ip {
                ip,
                country: counts.last_country,
                proxied: counts.last_proxied.unwrap_or_default(),
            }),
            ip_count: counts.ip_count,
        },
        counts: Counts {
            ips: counts.ip_count,
            agents: counts.agent_count,
            links: counts.links,
            authored_tags: counts.authored_tags,
            reviewed_tags: counts.reviewed_tags,
            reviews: counts.reviews,
            votes: counts.votes,
            admin_actions: counts.admin_actions,
        },
        labels: Labels::default(),
    }
}

async fn developer(state: &AppState, member_id: Option<i64>) -> Result<Option<DevKey>, AdminError> {
    let Some(member_id) = member_id else {
        return Ok(None);
    };
    developer_key(state, member_id).await
}

pub async fn developer_key(state: &AppState, member_id: i64) -> Result<Option<DevKey>, AdminError> {
    sqlx::query_as(
        "SELECT k.id AS key_id, d.api_key AS key, d.label, d.permissions, d.rate_limit,
                d.request_count, d.locked, d.created_at
         FROM developer_keys d
         LEFT JOIN member_api_keys k ON k.api_key = d.api_key
         WHERE d.member_id = $1",
    )
    .bind(member_id)
    .fetch_optional(state.db.pool())
    .await
    .context("read the developer key")
}

async fn minecraft_links(
    state: &AppState,
    member_id: Option<i64>,
) -> Result<Vec<LinkRow>, AdminError> {
    let Some(member_id) = member_id else {
        return Ok(Vec::new());
    };
    sqlx::query_as(
        "SELECT l.uuid, l.relation, l.linked_at,
                ARRAY(SELECT m.discord_id FROM members m WHERE m.uuid = l.uuid AND m.id <> l.member_id ORDER BY m.id) AS shared_discord_ids,
                ARRAY(SELECT m.id FROM members m WHERE m.uuid = l.uuid AND m.id <> l.member_id ORDER BY m.id) AS shared_member_ids
         FROM member_mc_links l
         WHERE l.member_id = $1 AND l.unlinked_at IS NULL
         ORDER BY l.relation = 'primary' DESC, l.linked_at DESC NULLS LAST, l.id DESC",
    )
    .bind(member_id)
    .fetch_all(state.db.pool())
    .await
    .context("read the account's Minecraft links")
}

async fn strikes_and_standing(
    state: &AppState,
    member: Option<&Member>,
) -> Result<(Vec<Strike>, Option<StandingView>), AdminError> {
    let Some(member) = member else {
        return Ok((Vec::new(), None));
    };
    let pool = state.db.pool();
    let (strikes, rules) = tokio::try_join!(
        async {
            StrikeRepository::new(pool)
                .list_for_member(member.discord_id)
                .await
                .context("read the account's strikes")
        },
        async {
            ReviewRulesRepository::new(pool)
                .current()
                .await
                .context("read the review rules")
        },
    )?;
    let explanation = standing::explain(member, &rules);
    let view = StandingView {
        can_vote: explanation.can_vote,
        vote_reason: explanation.vote_reason,
        can_tag: explanation.can_tag,
        tag_reason: explanation.tag_reason,
        effective_level: standing::effective_level(member, &rules),
        strike_count: standing::strike_count(member),
        accepted_tags: member.accepted_tags,
        rejected_tags: member.rejected_tags,
        accurate_verdicts: member.accurate_verdicts,
        incorrect_verdicts: member.incorrect_verdicts,
        bonus_verdicts: member.bonus_verdicts,
    };
    Ok((strikes, Some(view)))
}

async fn starfish_summary(
    state: &AppState,
    discord_id: i64,
) -> Result<Option<StarfishSummary>, AdminError> {
    sqlx::query_as(
        "SELECT u.id AS user_id, u.license_status, u.github_username,
                EXISTS (SELECT 1 FROM starfish_sessions s WHERE s.user_id = u.id AND s.expires_at > now()) AS active_session
         FROM starfish_users u WHERE u.discord_id = $1",
    )
    .bind(discord_id)
    .fetch_optional(state.db.pool())
    .await
    .context("read the account's Starfish user")
}

async fn activity_totals(
    state: &AppState,
    member_id: Option<i64>,
    discord_id: i64,
    now: DateTime<Utc>,
) -> Result<ActivityRow, AdminError> {
    sqlx::query_as(
        "SELECT
            coalesce(sum(requests) FILTER (WHERE hour >= $3), 0)::int8 AS requests_24h,
            coalesce(sum(requests) FILTER (WHERE hour >= $3 AND cause = $5), 0)::int8 AS caller_24h,
            coalesce(sum(requests) FILTER (WHERE hour >= $3 AND cause = $6), 0)::int8 AS upstream_24h,
            coalesce(sum(requests) FILTER (WHERE hour >= $3 AND status >= $7 AND cause NOT IN ($5, $6)), 0)::int8 AS errors_24h,
            coalesce(sum(subjects) FILTER (WHERE hour >= $3), 0)::int8 AS mentions_24h,
            coalesce(sum(requests), 0)::int8 AS requests_14d,
            coalesce(sum(requests) FILTER (WHERE cause = $5), 0)::int8 AS caller_14d,
            coalesce(sum(requests) FILTER (WHERE cause = $6), 0)::int8 AS upstream_14d,
            coalesce(sum(requests) FILTER (WHERE status >= $7 AND cause NOT IN ($5, $6)), 0)::int8 AS errors_14d
         FROM api_caller_hour
         WHERE (member_id = $1 OR actor_id = $2) AND hour >= $4",
    )
    .bind(member_id)
    .bind(discord_id)
    .bind(now - DAY)
    .bind(now - FORTNIGHT)
    .bind(Cause::CallerLimit.code())
    .bind(Cause::UpstreamLimit.code())
    .bind(FIRST_ERROR_STATUS)
    .fetch_one(state.db.pool())
    .await
    .context("read the account's request totals")
}

async fn last_request_at(
    state: &AppState,
    member_id: Option<i64>,
    discord_id: i64,
    attribution_from: Option<DateTime<Utc>>,
) -> Result<Option<DateTime<Utc>>, AdminError> {
    sqlx::query_scalar(
        "SELECT greatest(
            (SELECT ts FROM api_request_log WHERE member_id = $1 ORDER BY ts DESC LIMIT 1),
            (SELECT ts FROM api_request_log WHERE actor_id = $2 AND ts >= $3 ORDER BY ts DESC LIMIT 1))",
    )
    .bind(member_id)
    .bind(discord_id)
    .bind(attribution_from)
    .fetch_one(state.db.pool())
    .await
    .context("read the account's newest request")
}

async fn counts(
    state: &AppState,
    member_id: Option<i64>,
    discord_id: i64,
    personal_key: Option<&str>,
) -> Result<CountRow, AdminError> {
    sqlx::query_as(
        "SELECT
            (SELECT id FROM member_api_keys WHERE member_id = $1 AND api_key = $4) AS personal_id,
            (SELECT count(*) FROM member_api_keys WHERE member_id = $1 AND retired_at IS NOT NULL) AS history_count,
            (SELECT count(*) FROM api_key_ips WHERE member_id = $1) AS ip_count,
            (SELECT count(*) FROM api_member_agents WHERE member_id = $1) AS agent_count,
            last.ip AS last_ip, last.country AS last_country, last.proxied AS last_proxied,
            (SELECT count(*) FROM member_mc_links WHERE member_id = $1) AS links,
            (SELECT count(*) FROM member_mc_links WHERE member_id = $1 AND unlinked_at IS NOT NULL) AS former_links,
            (SELECT count(*) FROM player_events WHERE author = $2 AND kind IN ('tag_set', 'tag_clear')) AS authored_tags,
            (SELECT count(*) FROM player_events WHERE $2 = ANY(reviewed_by)) AS reviewed_tags,
            (SELECT count(*) FROM review WHERE submitter_id = $2) AS reviews,
            (SELECT count(*) FROM review_vote WHERE voter_id = $2) AS votes,
            (SELECT count(*) FROM admin_actions WHERE target = $3 OR actor = $2) AS admin_actions
         FROM (SELECT 1) AS one
         LEFT JOIN LATERAL (
            SELECT host(ip_address) AS ip, country, proxied FROM api_key_ips
            WHERE member_id = $1 ORDER BY last_seen DESC LIMIT 1
         ) AS last ON true",
    )
    .bind(member_id)
    .bind(discord_id)
    .bind(discord_id.to_string())
    .bind(personal_key)
    .fetch_one(state.db.pool())
    .await
    .context("count the account's records")
}

fn minecraft_link(row: LinkRow) -> MinecraftLink {
    MinecraftLink {
        player: player_ref(&row.uuid),
        relation: row.relation,
        linked_at: row.linked_at,
        also_primary_of: row
            .shared_discord_ids
            .into_iter()
            .zip(row.shared_member_ids)
            .map(|(discord_id, member_id)| EntityRef::Account {
                discord_id,
                member_id: Some(member_id),
            })
            .collect(),
    }
}

pub fn player_ref(undashed: &str) -> EntityRef {
    match uuid::Uuid::try_parse(undashed) {
        Ok(uuid) => EntityRef::Player { uuid },
        Err(_) => EntityRef::Name {
            name: undashed.to_string(),
        },
    }
}

pub fn strike_row(strike: Strike) -> StrikeRow {
    let status = match strike.status_at(Utc::now()) {
        StrikeStatus::Active => "active",
        StrikeStatus::Expired => "expired",
        StrikeStatus::Revoked => "revoked",
    };
    StrikeRow {
        id: strike.id,
        reason: strike.reason,
        struck_by: account_ref(strike.struck_by),
        issued_at: strike.issued_at,
        expires_at: strike.expires_at,
        revoked_at: strike.revoked_at,
        revoked_by: strike.revoked_by.map(account_ref),
        status,
    }
}

pub fn account_ref(discord_id: i64) -> EntityRef {
    EntityRef::Account {
        discord_id,
        member_id: None,
    }
}

fn mentioned(core: &AccountCore) -> Vec<&EntityRef> {
    let links = core
        .minecraft
        .iter()
        .flat_map(|link| std::iter::once(&link.player).chain(&link.also_primary_of));
    let strikes = core
        .strikes
        .items
        .iter()
        .flat_map(|strike| std::iter::once(&strike.struck_by).chain(&strike.revoked_by));
    std::iter::once(&core.account)
        .chain(links)
        .chain(strikes)
        .collect()
}
