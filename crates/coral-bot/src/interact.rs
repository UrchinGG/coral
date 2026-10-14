use anyhow::Result;
use serenity::all::*;

use crate::commands::blacklist::channel::COLOR_ERROR;
use crate::commands::blacklist::review;
use crate::utils::text;

pub const CUSTOM_ID_SEPARATOR: char = ':';

pub fn section_text(s: &str) -> CreateSectionComponent<'static> {
    CreateSectionComponent::TextDisplay(CreateTextDisplay::new(s.to_string()))
}

pub fn parse_id(custom_id: &str) -> Option<u64> {
    let (_, payload) = custom_id.split_once(CUSTOM_ID_SEPARATOR)?;
    leading_segment(payload).parse().ok()
}

pub fn parse_ids(custom_id: &str) -> Option<(u64, String)> {
    let (_, payload) = custom_id.split_once(CUSTOM_ID_SEPARATOR)?;
    let (id, rest) = payload.split_once(CUSTOM_ID_SEPARATOR)?;
    Some((id.parse().ok()?, rest.to_string()))
}

pub fn custom_id_name(custom_id: &str) -> &str {
    let name_end = match custom_id.strip_prefix(review::ID_PREFIX) {
        Some(review_payload) => review::ID_PREFIX.len() + leading_segment(review_payload).len(),
        None => leading_segment(custom_id).len(),
    };
    &custom_id[..name_end]
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

pub async fn edit_component(
    ctx: &Context,
    component: &ComponentInteraction,
    components: Vec<CreateComponent<'static>>,
) -> Result<()> {
    component
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new()
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .components(components),
        )
        .await?;
    Ok(())
}

pub async fn followup_component_error(
    ctx: &Context,
    component: &ComponentInteraction,
    title: &str,
    description: &str,
) -> Result<()> {
    component
        .create_followup(
            &ctx.http,
            CreateInteractionResponseFollowup::new()
                .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                .components(vec![error_container(title, description)]),
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

pub fn error_container(title: &str, description: &str) -> CreateComponent<'static> {
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

fn leading_segment(custom_id: &str) -> &str {
    custom_id
        .split_once(CUSTOM_ID_SEPARATOR)
        .map_or(custom_id, |(segment, _)| segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET_ID: u64 = 123_456_789_012_345_678;
    const REVIEW_ID: i64 = 42;
    const UUID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn ids_parse_from_the_segments_after_the_name() {
        assert_eq!(
            parse_id(&format!("manage_lock:{TARGET_ID}")),
            Some(TARGET_ID)
        );
        assert_eq!(
            parse_id(&format!("manage_force_link:{TARGET_ID}:{UUID}")),
            Some(TARGET_ID)
        );
        assert_eq!(parse_id("help_button"), None);
        assert_eq!(
            parse_ids(&format!("manage_force_link:{TARGET_ID}:{UUID}")),
            Some((TARGET_ID, UUID.to_string()))
        );
        assert_eq!(parse_ids(&format!("manage_lock:{TARGET_ID}")), None);
    }

    #[test]
    fn custom_id_names_drop_ids_and_typed_text() {
        assert_eq!(custom_id_name("guild_pg:2:some guild"), "guild_pg");
        assert_eq!(
            custom_id_name(&format!("manage_lock:{TARGET_ID}")),
            "manage_lock"
        );
        assert_eq!(custom_id_name("help_button"), "help_button");
    }

    #[test]
    fn review_custom_id_names_keep_the_action() {
        assert_eq!(
            custom_id_name(&review::custom_id("accept", REVIEW_ID)),
            "rv:accept"
        );
        assert_eq!(
            custom_id_name(&review::custom_id_with("switch", REVIEW_ID, "reject")),
            "rv:switch"
        );
    }
}
