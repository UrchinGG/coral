use std::collections::HashMap;

use database::Database;

use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberIds(HashMap<i64, i64>);

impl MemberIds {
    pub async fn load(db: &Database, discord_ids: &[i64]) -> Result<Self, AdminError> {
        if discord_ids.is_empty() {
            return Ok(Self::default());
        }
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT discord_id, id FROM members WHERE discord_id = ANY($1)")
                .bind(discord_ids)
                .fetch_all(db.pool())
                .await
                .context("read the member ids of the mentioned accounts")?;
        Ok(Self(rows.into_iter().collect()))
    }

    pub fn account(&self, discord_id: i64) -> EntityRef {
        EntityRef::Account {
            discord_id,
            member_id: self.0.get(&discord_id).copied(),
        }
    }
}
