use serenity::all::{Context, Message};

use crate::framework::Data;

pub async fn on_message(ctx: &Context, data: &Data, message: &Message) {
    if message.author.bot() {
        return;
    }

    let Some(submitter_id) = data.assembling_submitter(message.channel_id.get()) else {
        return;
    };
    if message.author.id.get() == submitter_id {
        return;
    }

    if let Err(err) = message.delete(&ctx.http, None).await {
        tracing::warn!(
            thread = message.channel_id.get(),
            "failed to delete message in assembling review: {err}"
        );
    }
}
