use std::collections::{BTreeSet, HashMap};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{AdminError, Context};
use crate::names::LabelRequest;
use crate::refs::{EntityRef, Labels};
use crate::state::AppState;

pub const DEFAULT_PAGE_SIZE: i64 = 50;
pub const MAX_PAGE_SIZE: i64 = 200;

const KEY_BYTES: usize = 8;
const BYTEWISE_COLLATION: &str = "\"C\"";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowCursor {
    pub key: i64,
    pub tie: String,
}

#[derive(Debug, Serialize)]
pub struct RowPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<RowCursor>,
    pub labels: Labels,
}

#[derive(Debug, Default)]
pub struct Mentions {
    discord: BTreeSet<i64>,
    minecraft: BTreeSet<Uuid>,
}

pub struct Accounts {
    members: HashMap<i64, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainsText {
    pattern: String,
    ascii: bool,
}

impl RowCursor {
    pub fn at(ts: DateTime<Utc>, id: i64) -> Self {
        Self {
            key: ts.timestamp_micros(),
            tie: id.to_string(),
        }
    }

    pub fn ranked(key: i64, tie: impl Into<String>) -> Self {
        Self {
            key,
            tie: tie.into(),
        }
    }

    pub fn ts(&self) -> Result<DateTime<Utc>, AdminError> {
        DateTime::from_timestamp_micros(self.key)
            .ok_or_else(|| AdminError::BadRequest("the cursor holds no valid time".into()))
    }

    pub fn id(&self) -> Result<i64, AdminError> {
        self.tie
            .parse()
            .map_err(|_| AdminError::BadRequest("the cursor holds no valid row id".into()))
    }

    fn encode(&self) -> String {
        let mut bytes = self.key.to_be_bytes().to_vec();
        bytes.extend_from_slice(self.tie.as_bytes());
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn decode(raw: &str) -> Result<Self, AdminError> {
        let invalid = || AdminError::BadRequest(format!("cursor {raw:?} is not a valid cursor"));
        let bytes = URL_SAFE_NO_PAD.decode(raw).map_err(|_| invalid())?;
        if bytes.len() < KEY_BYTES {
            return Err(invalid());
        }
        let (key, tie) = bytes.split_at(KEY_BYTES);
        let key = i64::from_be_bytes(key.try_into().map_err(|_| invalid())?);
        let tie = String::from_utf8(tie.to_vec()).map_err(|_| invalid())?;
        Ok(Self { key, tie })
    }
}

impl Serialize for RowCursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for RowCursor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::decode(&raw).map_err(serde::de::Error::custom)
    }
}

impl Mentions {
    pub fn account(&mut self, discord_id: i64) {
        self.discord.insert(discord_id);
    }

    pub fn player(&mut self, uuid: Uuid) {
        self.minecraft.insert(uuid);
    }

    pub fn discord_ids(&self) -> Vec<i64> {
        self.discord.iter().copied().collect()
    }

    pub async fn labels(&self, state: &AppState) -> Result<Labels, AdminError> {
        let request = LabelRequest {
            discord: self.discord_ids(),
            minecraft: self
                .minecraft
                .iter()
                .map(|uuid| uuid.simple().to_string())
                .collect(),
        };
        state.names.cached(&request).await
    }
}

impl Accounts {
    pub fn empty() -> Self {
        Self {
            members: HashMap::new(),
        }
    }

    pub async fn load(state: &AppState, discord_ids: &[i64]) -> Result<Self, AdminError> {
        if discord_ids.is_empty() {
            return Ok(Self::empty());
        }
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT discord_id, id FROM members WHERE discord_id = ANY($1)")
                .bind(discord_ids)
                .fetch_all(state.db.pool())
                .await
                .context("read the member ids of mentioned accounts")?;
        Ok(Self {
            members: rows.into_iter().collect(),
        })
    }

    pub fn of(&self, discord_id: i64) -> EntityRef {
        EntityRef::Account {
            discord_id,
            member_id: self.members.get(&discord_id).copied(),
        }
    }
}

impl ContainsText {
    pub fn new(text: &str) -> Self {
        Self {
            pattern: contains_pattern(text),
            ascii: text.is_ascii(),
        }
    }

    pub fn push_match(&self, sql: &mut QueryBuilder<'_, Postgres>, column: &str) {
        sql.push(self.matched_column(column))
            .push(" ILIKE ")
            .push_bind(self.pattern.clone());
    }

    fn matched_column(&self, column: &str) -> String {
        if !self.ascii {
            return column.to_string();
        }
        format!("({column} COLLATE {BYTEWISE_COLLATION})")
    }
}

pub fn page_size(requested: Option<i64>) -> Result<i64, AdminError> {
    let size = requested.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&size) {
        return Err(AdminError::BadRequest(format!(
            "limit must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    Ok(size)
}

pub fn split_page<T>(
    mut rows: Vec<T>,
    limit: i64,
    cursor_of: impl Fn(&T) -> RowCursor,
) -> (Vec<T>, Option<RowCursor>) {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    if rows.len() <= limit {
        return (rows, None);
    }
    rows.truncate(limit);
    let next = rows.last().map(cursor_of);
    (rows, next)
}

fn contains_pattern(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn cursors_round_trip_their_key_and_tie() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 14, 3, 14, 32).unwrap()
            + chrono::Duration::microseconds(7);
        let cursor = RowCursor::at(ts, 4242);
        let decoded = RowCursor::decode(&cursor.encode()).unwrap();
        assert_eq!(decoded, cursor);
        assert_eq!(decoded.ts().unwrap(), ts);
        assert_eq!(decoded.id().unwrap(), 4242);
        let ranked = RowCursor::ranked(-3, "5b3e8f1a9c2d4e6f7a8b9c0d");
        assert_eq!(RowCursor::decode(&ranked.encode()).unwrap(), ranked);
    }

    #[test]
    fn malformed_cursors_are_bad_requests() {
        for raw in ["", "!!", "AAAA"] {
            assert_eq!(RowCursor::decode(raw).unwrap_err().code(), "bad_request");
        }
    }

    #[test]
    fn a_full_page_carries_the_cursor_of_its_last_row() {
        let (rows, next) = split_page(vec![1, 2, 3], 2, |row| RowCursor::ranked(*row, "x"));
        assert_eq!(rows, vec![1, 2]);
        assert_eq!(next, Some(RowCursor::ranked(2, "x")));
        let (rows, next) = split_page(vec![1, 2], 2, |row| RowCursor::ranked(*row, "x"));
        assert_eq!(rows, vec![1, 2]);
        assert_eq!(next, None);
    }

    #[test]
    fn page_sizes_outside_the_allowed_range_are_rejected() {
        assert_eq!(page_size(None).unwrap(), DEFAULT_PAGE_SIZE);
        assert_eq!(page_size(Some(MAX_PAGE_SIZE)).unwrap(), MAX_PAGE_SIZE);
        assert!(page_size(Some(0)).is_err());
        assert!(page_size(Some(MAX_PAGE_SIZE + 1)).is_err());
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(contains_pattern("50%_off\\"), "%50\\%\\_off\\\\%");
    }

    #[test]
    fn ascii_needles_fold_case_bytewise_and_others_by_locale() {
        let ascii = ContainsText::new("Replay");
        assert_eq!(ascii.matched_column("e.reason"), "(e.reason COLLATE \"C\")");
        let accented = ContainsText::new("Élan");
        assert_eq!(accented.matched_column("e.reason"), "e.reason");
        let mut sql = QueryBuilder::<Postgres>::new("SELECT 1 WHERE ");
        ascii.push_match(&mut sql, "name");
        assert_eq!(sql.sql(), "SELECT 1 WHERE (name COLLATE \"C\") ILIKE $1");
    }
}
