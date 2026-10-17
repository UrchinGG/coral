use serenity::all::*;

use coral_redis::LinkMethod;
use database::CacheRepository;

use crate::framework::Data;

pub async fn handle_takeover(
    ctx: Context,
    data: Data,
    uuid: String,
    previous_discord_id: u64,
    new_discord_id: u64,
    method: LinkMethod,
) {
    let ign = CacheRepository::new(data.db.pool())
        .get_username(&uuid)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| uuid.clone());
    let new_user = UserId::new(new_discord_id)
        .to_user(&ctx.http)
        .await
        .map(|u| format!("<@{new_discord_id}> (`{}`)", u.name))
        .unwrap_or_else(|_| format!("<@{new_discord_id}>"));

    let content = format!(
        "**{ign}** was just verified by another Discord account, {new_user}, {}.\n\n\
         A Minecraft account can only be verified for one person in the Urchin server, so it \
         now belongs to them there, and your linked roles and nickname for it have been removed. \
         It is still linked to your account for everything else.\n\n\
         If this wasn't you sharing the account, re-verify it to take it back and secure the \
         account, or contact staff.",
        describe(method),
    );

    let sent = match UserId::new(previous_discord_id)
        .create_dm_channel(&ctx.http)
        .await
    {
        Ok(channel) => ctx
            .http
            .send_message(
                channel.id.into(),
                Vec::new(),
                &CreateMessage::new().content(content),
            )
            .await
            .is_ok(),
        Err(_) => false,
    };
    if !sent {
        tracing::debug!("Could not DM {previous_discord_id} about takeover of {uuid}");
    }

    crate::sync::sync_user(ctx, data, UserId::new(previous_discord_id)).await;
}

fn describe(method: LinkMethod) -> &'static str {
    match method {
        LinkMethod::HypixelSocial => {
            "because the account's Hypixel social settings were set to their Discord username"
        }
        LinkMethod::VerificationServer => {
            "by joining `link.urchin.gg` with the account and entering the code it gave"
        }
        LinkMethod::Staff => "after a staff member linked it to them manually",
    }
}
