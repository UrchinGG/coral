use database::AdminActionRepository;
use serde_json::Value;

use crate::state::AppState;

pub async fn log(state: &AppState, actor: i64, action: &str, target: &str, details: Value) {
    let repo = AdminActionRepository::new(state.db.pool());
    if let Err(e) = repo.log(actor, action, target, &details).await {
        tracing::warn!("failed to record admin action {action} on {target}: {e}");
    }
}
