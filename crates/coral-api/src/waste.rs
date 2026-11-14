use chrono::Utc;
use coral_redis::waste::{self, Charge};
use reqlog::{KeyKind, current_caller};

use crate::state::AppState;

pub fn calling_personal_key() -> Option<i64> {
    let caller = current_caller()?;
    match caller.key_kind {
        KeyKind::Personal => caller.member_id,
        _ => None,
    }
}

pub async fn charge_wasted_call(state: &AppState, personal_key: Option<i64>) {
    let Some(member) = personal_key else {
        return;
    };
    match waste::charge(&mut state.redis.connection(), member, Utc::now()).await {
        Ok(Charge::Blocked) => {
            tracing::warn!("blocked member {member}'s personal key for wasting Hypixel calls")
        }
        Ok(Charge::Within) => {}
        Err(e) => tracing::warn!("failed to charge a wasted Hypixel call to member {member}: {e}"),
    }
}

pub async fn is_blocked(state: &AppState, member: i64) -> bool {
    waste::is_blocked(&mut state.redis.connection(), member)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "failed to check whether member {member}'s personal key is blocked: {e}"
            );
            false
        })
}
