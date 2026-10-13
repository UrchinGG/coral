use chrono::{DateTime, Duration, Utc};
use serenity::all::*;

use database::{Strike, StrikeRepository};

use crate::commands::blacklist::{appeal, channel};
use crate::framework::Data;

const TICKET_URL: &str =
    "https://discord.com/channels/1339318572069158962/1342235914746986556/1409941039824769034";
pub const DURATION_CHOICES: &[(&str, i64)] = &[("14 days", 14), ("30 days", 30)];
pub const REASON_MAX_CHARS: u16 = 100;

pub struct IssuedStrike {
    pub strike: Strike,
    pub dm_delivered: bool,
}

pub fn expiry_from_days(days: Option<i64>) -> Option<DateTime<Utc>> {
    days.filter(|d| DURATION_CHOICES.iter().any(|(_, allowed)| allowed == d))
        .map(|d| Utc::now() + Duration::days(d))
}

pub async fn issue(
    ctx: &Context,
    data: &Data,
    target_id: u64,
    reason: &str,
    struck_by: u64,
    expires_at: Option<DateTime<Utc>>,
    source_ref: Option<&str>,
) -> anyhow::Result<Option<IssuedStrike>> {
    let Some(strike) = StrikeRepository::new(data.db.pool())
        .issue(
            target_id as i64,
            reason,
            struck_by as i64,
            expires_at,
            source_ref,
        )
        .await?
    else {
        return Ok(None);
    };
    let dm_delivered = after_issue(ctx, data, &strike).await;
    Ok(Some(IssuedStrike {
        strike,
        dm_delivered,
    }))
}

pub async fn revoke(
    ctx: &Context,
    data: &Data,
    strike_id: i64,
    revoked_by: u64,
) -> anyhow::Result<Option<Strike>> {
    let revoked = StrikeRepository::new(data.db.pool())
        .revoke(strike_id, revoked_by as i64)
        .await?;
    if let Some(strike) = &revoked {
        after_revoke(ctx, data, strike).await;
    }
    Ok(revoked)
}

pub async fn after_issue(ctx: &Context, data: &Data, strike: &Strike) -> bool {
    let target_id = strike.discord_id as u64;
    crate::utils::standing::refresh_and_sync(data, target_id).await;
    if let Some(expires_at) = strike.expires_at {
        schedule_expiry(data.clone(), strike.id, expires_at);
    }
    channel::post_strike_issued(ctx, data, strike).await;
    appeal::send_dm(ctx, UserId::new(target_id), &strike_dm(strike)).await
}

pub async fn after_revoke(ctx: &Context, data: &Data, strike: &Strike) {
    crate::utils::standing::refresh_and_sync(data, strike.discord_id as u64).await;
    channel::post_strike_revoked(ctx, data, strike).await;
}

pub async fn hydrate_strikes(data: Data) {
    let repo = StrikeRepository::new(data.db.pool());
    match repo.import_unrecorded().await {
        Ok(0) => {}
        Ok(imported) => tracing::info!("imported {imported} legacy strikes"),
        Err(e) => tracing::error!("failed to import legacy strikes: {e}"),
    }
    match repo.with_pending_expiry().await {
        Ok(strikes) => {
            let count = strikes.len();
            for strike in strikes {
                if let Some(expires_at) = strike.expires_at {
                    schedule_expiry(data.clone(), strike.id, expires_at);
                }
            }
            tracing::info!("scheduled expiry for {count} strikes");
        }
        Err(e) => tracing::error!("failed to hydrate strike expiries: {e}"),
    }
}

fn schedule_expiry(data: Data, strike_id: i64, expires_at: DateTime<Utc>) {
    tokio::spawn(async move {
        let delay = (expires_at - Utc::now()).to_std().unwrap_or_default();
        tokio::time::sleep(delay).await;
        match StrikeRepository::new(data.db.pool())
            .settle_expiry(strike_id)
            .await
        {
            Ok(Some(strike)) => {
                crate::utils::standing::refresh_and_sync(&data, strike.discord_id as u64).await;
            }
            Ok(None) => {}
            Err(e) => tracing::error!("failed to settle strike {strike_id} expiry: {e}"),
        }
    });
}

fn strike_dm(strike: &Strike) -> String {
    let opening = match strike.expires_at {
        Some(at) => format!(
            "You have been given a strike by staff. It will expire on <t:{}:f>.",
            at.timestamp()
        ),
        None => "You have been given a strike by staff.".to_string(),
    };
    format!(
        "{opening}\n\n> {}\n\n\
         While a strike is active on your account, you cannot earn tagging or voting \
         permissions. After two active strikes, any existing permissions you have are \
         suspended. Use `/dashboard` to view your standing and active strikes. If you \
         believe your strike should be removed, please [open a ticket]({TICKET_URL}).",
        crate::utils::sanitize_reason(&strike.reason)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_preset_durations_set_an_expiry() {
        assert!(expiry_from_days(None).is_none());
        assert!(expiry_from_days(Some(7)).is_none());
        assert!(expiry_from_days(Some(14)).is_some());
        assert!(expiry_from_days(Some(30)).is_some());
    }
}
