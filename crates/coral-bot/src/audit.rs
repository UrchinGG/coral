use std::fmt::Display;

use serde_json::Value;

use database::AdminActionRepository;

use crate::framework::Data;

pub async fn record(data: &Data, actor: u64, action: &str, target: impl Display, details: Value) {
    let target = target.to_string();
    if let Err(e) = AdminActionRepository::new(data.db.pool())
        .log(actor as i64, action, &target, &details)
        .await
    {
        tracing::error!("Failed to record {action} by {actor} on {target}: {e}");
    }
}
