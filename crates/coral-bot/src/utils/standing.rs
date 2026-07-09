use serenity::all::*;

use database::{MemberRepository, standing};

use crate::framework::Data;

pub async fn refresh_and_sync(ctx: &Context, data: &Data, discord_id: u64) {
    let repo = MemberRepository::new(data.db.pool());
    match standing::refresh(&repo, discord_id as i64).await {
        Ok(change) => {
            if let Some(granted) = change.tag {
                sync_trusted_role(ctx, data, discord_id, granted).await;
            }
        }
        Err(e) => tracing::error!("Failed to refresh standing for {discord_id}: {e}"),
    }
}

async fn sync_trusted_role(ctx: &Context, data: &Data, discord_id: u64, granted: bool) {
    let (Some(guild_id), Some(role_id)) = (data.home_guild_id, data.trusted_role_id) else {
        return;
    };
    let user_id = UserId::new(discord_id);
    let result = if granted {
        ctx.http
            .add_member_role(guild_id, user_id, role_id, Some("Earned tagging standing"))
            .await
    } else {
        ctx.http
            .remove_member_role(guild_id, user_id, role_id, Some("Lost tagging standing"))
            .await
    };
    if let Err(e) = result {
        tracing::warn!("Failed to sync trusted role for {discord_id}: {e}");
    }
}
