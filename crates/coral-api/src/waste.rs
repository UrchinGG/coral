use chrono::Utc;
use coral_redis::waste::{self, Charge, KeyHolder};
use reqlog::{KeyKind, current_caller};

use crate::state::AppState;

pub fn calling_key() -> Option<KeyHolder> {
    let caller = current_caller()?;
    let member = caller.member_id?;
    match caller.key_kind {
        KeyKind::Personal => Some(KeyHolder::Personal(member)),
        KeyKind::Developer => Some(KeyHolder::Developer(member)),
        _ => None,
    }
}

pub async fn charge_wasted_call(state: &AppState, holder: Option<KeyHolder>) {
    let Some(holder) = holder else {
        return;
    };
    match waste::charge(&mut state.redis.connection(), holder, Utc::now()).await {
        Ok(Charge::Blocked) => tracing::warn!("blocked {holder} for wasting Hypixel calls"),
        Ok(Charge::Within) => {}
        Err(e) => tracing::warn!("failed to charge a wasted Hypixel call to {holder}: {e}"),
    }
}

pub async fn is_blocked(state: &AppState, holder: KeyHolder) -> bool {
    waste::is_blocked(&mut state.redis.connection(), holder)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("failed to check whether {holder} is blocked: {e}");
            false
        })
}
