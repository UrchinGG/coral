use serenity::all::{Context, Message};

use crate::framework::Data;

pub async fn on_message(ctx: &Context, data: &Data, message: &Message) {
    if message.author.bot() {
        return;
    }

    // The guide thread stays unlocked so its opt-in button keeps working, so
    // the bot has to keep it clear of replies itself.
    if data.guide_thread() == Some(message.channel_id.get()) {
        delete(ctx, message, "review guide thread").await;
    }
}

async fn delete(ctx: &Context, message: &Message, context: &str) {
    if let Err(err) = message.delete(&ctx.http, None).await {
        tracing::warn!(
            thread = message.channel_id.get(),
            "failed to delete message in {context}: {err}"
        );
    }
}
