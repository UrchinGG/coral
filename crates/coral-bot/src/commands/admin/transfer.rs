use anyhow::{Result, anyhow};
use serenity::all::*;

use coral_redis::SyncEvent;
use database::{TransferError, TransferPreview, TransferRepository};

use crate::{framework::Data, interact, utils::text};

use super::manage::{build_main_view, fetch_context};

const SOURCE_FIELD: &str = "source";

pub async fn handle_open(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    if !data.is_owner(component.user.id.get()) {
        return deny_component(ctx, component).await;
    }
    let target_id = target_from(&component.data.custom_id)?;
    let source_select = CreateSelectMenu::new(
        SOURCE_FIELD,
        CreateSelectMenuKind::User {
            default_users: None,
        },
    )
    .required(true);
    let modal = CreateModal::new(
        format!("manage_transfer_modal:{target_id}"),
        "Transfer Data",
    )
    .components(vec![CreateModalComponent::Label(
        CreateLabel::select_menu("Source account", source_select).description(
            "Its data moves onto this member, and the source account is then deleted.",
        ),
    )]);
    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_modal(ctx: &Context, modal: &ModalInteraction, data: &Data) -> Result<()> {
    if !data.is_owner(modal.user.id.get()) {
        return interact::send_modal_error(ctx, modal, "Error", "Only the owner can transfer data")
            .await;
    }
    let target_id = target_from(&modal.data.custom_id)?;
    let source_id = selected_user(&modal.data.components).ok_or_else(|| anyhow!("No source"))?;

    let preview = TransferRepository::new(data.db.pool())
        .preview(source_id as i64, target_id as i64)
        .await;
    let components = match preview {
        Ok(preview) => {
            let target_name = display_name(ctx, target_id).await;
            preflight_view(source_id, target_id, &target_name, preview)
        }
        Err(e) => transfer_error_view(e, source_id, target_id),
    };
    interact::update_modal(ctx, modal, components).await
}

pub async fn handle_confirm(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let invoker_id = component.user.id.get();
    if !data.is_owner(invoker_id) {
        return deny_component(ctx, component).await;
    }
    let (target_id, source_raw) =
        interact::parse_ids(&component.data.custom_id).ok_or_else(|| anyhow!("Invalid ID"))?;
    let source_id: u64 = source_raw.parse()?;

    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let result = TransferRepository::new(data.db.pool())
        .transfer(source_id as i64, target_id as i64, invoker_id as i64)
        .await;
    let components = match result {
        Ok(_) => {
            finish_transfer(ctx, data, source_id, target_id).await;
            result_view(source_id, target_id)
        }
        Err(e) => transfer_error_view(e, source_id, target_id),
    };
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

pub async fn handle_cancel(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let invoker_id = component.user.id.get();
    if !data.is_owner(invoker_id) {
        return deny_component(ctx, component).await;
    }
    let target_id = target_from(&component.data.custom_id)?;
    let (invoker_rank, _, _) = fetch_context(data, invoker_id, target_id).await?;
    interact::update_message(
        ctx,
        component,
        build_main_view(data, invoker_rank, target_id).await,
    )
    .await
}

async fn finish_transfer(ctx: &Context, data: &Data, source_id: u64, target_id: u64) {
    crate::utils::standing::refresh_and_sync(ctx, data, target_id).await;
    for discord_id in [source_id, target_id] {
        data.sync_event_publisher
            .publish(&SyncEvent::SyncUser { discord_id })
            .await;
    }
}

fn preflight_view(
    source_id: u64,
    target_id: u64,
    target_name: &str,
    preview: TransferPreview,
) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(format!(
            "## Transfer Data\n\
             All data on <@{source_id}> will move to <@{target_id}>, and <@{source_id}>'s \
             account will be deleted.\n\
             -# Tags authored: {}\n\
             -# Linked accounts: {}",
            preview.tags_authored, preview.linked_accounts,
        )),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
            CreateButton::new(format!("manage_transfer_go:{target_id}:{source_id}"))
                .label(format!("Transfer to {target_name}"))
                .style(ButtonStyle::Danger),
            CreateButton::new(format!("manage_transfer_cancel:{target_id}"))
                .label("Cancel")
                .style(ButtonStyle::Secondary),
        ])),
    ]))]
}

fn result_view(source_id: u64, target_id: u64) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(format!(
            "You transferred <@{source_id}>'s data to <@{target_id}>. \
             <@{source_id}>'s account was deleted."
        )),
    ]))]
}

fn transfer_error_view(
    error: TransferError,
    source_id: u64,
    target_id: u64,
) -> Vec<CreateComponent<'static>> {
    let message = match error {
        TransferError::SameAccount => {
            "Choose a different account than the one you are transferring onto."
        }
        TransferError::NothingToTransfer => "That account has no data to transfer.",
        TransferError::Database(e) => {
            tracing::error!("Transfer from {source_id} to {target_id} failed: {e}");
            "The transfer failed and nothing was changed."
        }
    };
    vec![interact::error_container("Error", message)]
}

async fn deny_component(ctx: &Context, component: &ComponentInteraction) -> Result<()> {
    interact::send_component_error(ctx, component, "Error", "Only the owner can transfer data")
        .await
}

fn target_from(custom_id: &str) -> Result<u64> {
    interact::parse_id(custom_id).ok_or_else(|| anyhow!("Invalid ID"))
}

fn selected_user(components: &[Component]) -> Option<u64> {
    components.iter().find_map(|component| match component {
        Component::Label(label) => match &label.component {
            LabelComponent::SelectMenu(menu) if menu.custom_id == SOURCE_FIELD => {
                menu.values.first()?.parse().ok()
            }
            _ => None,
        },
        _ => None,
    })
}

async fn display_name(ctx: &Context, user_id: u64) -> String {
    match UserId::new(user_id).to_user(ctx).await {
        Ok(user) => user.name.to_string(),
        Err(_) => user_id.to_string(),
    }
}
