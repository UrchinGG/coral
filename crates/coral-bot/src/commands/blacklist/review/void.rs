use anyhow::Result;
use serenity::all::*;

use database::{Review, ReviewError, ReviewRepository, ReviewStatus};

use super::{custom_id, render};
use crate::audit;
use crate::commands::blacklist::channel::tag_label;
use crate::framework::{AccessRank, Data};
use crate::interact;
use crate::utils::text;

pub const COMMAND_NAME: &str = "void";

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new(COMMAND_NAME)
        .description("Void the tag review in this thread")
        .integration_types(vec![InstallationContext::Guild])
        .contexts(vec![InteractionContext::Guild])
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    let review = match check(data, command.user.id.get(), command.channel_id.get() as i64).await? {
        Ok(review) => review,
        Err(refusal) => return interact::send_error(ctx, command, "Error", refusal).await,
    };
    let player = player_name(ctx, data, &review).await?;
    let mut parts = vec![text(format!(
        "Void this review of `{player}`? It will close without applying the tag, and nobody will be scored."
    ))];
    if review.hide_username {
        parts.push(text(format!(
            "-# Submitted anonymously by <@{}>.",
            review.submitter_id
        )));
    }
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(custom_id("void_go", review.id))
                .label("Void review")
                .style(ButtonStyle::Danger),
        ]),
    ));
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(vec![CreateComponent::Container(CreateContainer::new(
                        parts,
                    ))]),
            ),
        )
        .await?;
    Ok(())
}

pub async fn confirm(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    review_id: i64,
) -> Result<()> {
    let staff = component.user.id.get();
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    if super::super::tag::get_rank(data, staff).await? < AccessRank::Helper {
        return interact::edit_component(
            ctx,
            component,
            render::simple("Only helpers and above can use /void"),
        )
        .await;
    }
    let repo = ReviewRepository::new(data.db.pool());
    let message = match repo.void(review_id, staff as i64).await {
        Ok(review) => {
            audit::record(
                data,
                staff,
                "void_review",
                review.id,
                serde_json::json!({ "player": review.player_uuid }),
            )
            .await;
            super::resolve::announce(ctx, data, &review, "voided", Some(staff)).await;
            super::resolve::close_out(ctx, data, &review).await;
            let label = match repo.live_revision(review.id).await? {
                Some(revision) => tag_label(&revision.tag_type),
                None => String::new(),
            };
            format!(
                "You **voided** the review of the {label} tag on `{}`.",
                player_name(ctx, data, &review).await?
            )
        }
        Err(ReviewError::WrongStatus(ReviewStatus::Voided)) => {
            "This review was already voided.".to_string()
        }
        Err(ReviewError::WrongStatus(_)) => "Only open reviews can be voided.".to_string(),
        Err(e) => return Err(anyhow::anyhow!("{e:?}")),
    };
    interact::edit_component(ctx, component, render::simple(message)).await
}

async fn check(data: &Data, user: u64, channel_id: i64) -> Result<Result<Review, &'static str>> {
    if super::super::tag::get_rank(data, user).await? < AccessRank::Helper {
        return Ok(Err("Only helpers and above can use /void"));
    }
    let Some(review) = ReviewRepository::new(data.db.pool())
        .get_by_thread(channel_id)
        .await?
    else {
        return Ok(Err(
            "Run /void inside the thread of the review you want to void.",
        ));
    };
    Ok(match review.status {
        ReviewStatus::Voided => Err("This review was already voided."),
        ReviewStatus::Open => Ok(review),
        _ => Err("Only open reviews can be voided."),
    })
}

async fn player_name(ctx: &Context, data: &Data, review: &Review) -> Result<String> {
    Ok(super::load_live(ctx, data, review.id)
        .await?
        .map(|loaded| loaded.username)
        .unwrap_or_else(|| review.player_uuid.clone()))
}
