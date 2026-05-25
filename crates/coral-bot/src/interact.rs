use anyhow::Result;
use serenity::all::*;

use crate::commands::blacklist::channel::COLOR_ERROR;
use crate::utils::text;

pub fn section_text(s: &str) -> CreateSectionComponent<'static> {
    CreateSectionComponent::TextDisplay(CreateTextDisplay::new(s.to_string()))
}

pub fn parse_id(custom_id: &str) -> Option<u64> {
    custom_id
        .splitn(2, ':')
        .nth(1)?
        .split(':')
        .next()?
        .parse()
        .ok()
}

pub fn parse_ids(custom_id: &str) -> Option<(u64, String)> {
    let payload = custom_id.splitn(2, ':').nth(1)?;
    let mut parts = payload.splitn(2, ':');
    Some((parts.next()?.parse().ok()?, parts.next()?.to_string()))
}

pub fn parse_compound_id(custom_id: &str) -> Option<(u64, u64)> {
    let payload = custom_id.splitn(2, ':').nth(1)?;
    let mut parts = payload.split(':');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

pub fn extract_modal_value(components: &[Component], field_id: &str) -> String {
    for component in components {
        if let Component::Label(label) = component {
            if let LabelComponent::InputText(input) = &label.component {
                if input.custom_id == field_id {
                    return input.value.as_deref().unwrap_or("").to_string();
                }
            }
        }
    }
    String::new()
}

pub async fn update_message(
    ctx: &Context,
    component: &ComponentInteraction,
    components: Vec<CreateComponent<'static>>,
) -> Result<()> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(components),
            ),
        )
        .await?;
    Ok(())
}

pub async fn update_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    components: Vec<CreateComponent<'static>>,
) -> Result<()> {
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .components(components),
            ),
        )
        .await?;
    Ok(())
}

pub async fn send_error(
    ctx: &Context,
    command: &CommandInteraction,
    title: &str,
    description: &str,
) -> Result<()> {
    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(error_response(title, description)),
        )
        .await?;
    Ok(())
}

pub async fn send_deferred_error(
    ctx: &Context,
    command: &CommandInteraction,
    title: &str,
    description: &str,
) -> Result<()> {
    command
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(vec![error_container(title, description)]),
        )
        .await?;
    Ok(())
}

pub async fn send_component_error(
    ctx: &Context,
    component: &ComponentInteraction,
    title: &str,
    description: &str,
) -> Result<()> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(error_response(title, description)),
        )
        .await?;
    Ok(())
}

pub async fn send_modal_error(
    ctx: &Context,
    modal: &ModalInteraction,
    title: &str,
    description: &str,
) -> Result<()> {
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(error_response(title, description)),
        )
        .await?;
    Ok(())
}

fn error_container(title: &str, description: &str) -> CreateComponent<'static> {
    let body = if description.is_empty() {
        format!("## {title}")
    } else {
        format!("## {title}\n{description}")
    };
    CreateComponent::Container(CreateContainer::new(vec![text(body)]).accent_color(COLOR_ERROR))
}

fn error_response(title: &str, description: &str) -> CreateInteractionResponseMessage<'static> {
    CreateInteractionResponseMessage::new()
        .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
        .components(vec![error_container(title, description)])
}
