use anyhow::{Result, anyhow};
use serenity::all::*;

use database::MemberRepository;

use super::manage::fetch_context;
use crate::api::{ApiError, LinkProof, LinkRefusal, LinkedAccount, Links};
use crate::commands::blacklist::channel;
use crate::framework::{AccessRank, Data};
use crate::interact;
use crate::utils::{separator, text};

fn context_prefix(custom_id: &str) -> &'static str {
    if custom_id.starts_with("dashboard_") {
        "dashboard"
    } else if custom_id.starts_with("link_") {
        "link"
    } else {
        "manage"
    }
}

fn resolve_can_modify(
    prefix: &str,
    invoker_rank: AccessRank,
    target_rank: AccessRank,
    is_self: bool,
) -> bool {
    prefix == "dashboard"
        || is_self
        || (invoker_rank > target_rank && invoker_rank >= AccessRank::Moderator)
}

fn extract_select_value(component: &ComponentInteraction) -> Option<&str> {
    match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.first().map(|s| s.as_str()),
        _ => None,
    }
}

fn account_name(account: &LinkedAccount) -> &str {
    account.username.as_deref().unwrap_or(&account.uuid)
}

async fn build_accounts_view(
    data: &Data,
    can_modify: bool,
    target_id: u64,
    prefix: &str,
) -> Result<Vec<CreateComponent<'static>>> {
    let links = data.api.links(target_id).await?;
    if links.primary.is_none() && links.alts.is_empty() {
        return Ok(build_link_new_view(data, prefix, target_id).await);
    }

    let mut parts: Vec<CreateContainerComponent> = vec![text("### Linked Accounts")];
    build_primary_section(&mut parts, &links, can_modify, prefix, target_id);
    build_alt_sections(&mut parts, &links.alts, can_modify, prefix, target_id);
    build_accounts_actions(&mut parts, can_modify, prefix, target_id);

    Ok(vec![CreateComponent::Container(CreateContainer::new(
        parts,
    ))])
}

fn build_primary_section(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    links: &Links,
    can_modify: bool,
    prefix: &str,
    target_id: u64,
) {
    let Some(primary) = &links.primary else {
        parts.push(separator());
        parts.push(text("No account linked."));
        return;
    };
    let (uuid, name) = (&primary.uuid, account_name(primary));

    parts.push(separator());
    parts.push(text("**Primary**"));

    if links.alts.is_empty() {
        parts.push(text(format!("**`{name}`**")));
        parts.push(text(format!("-# UUID: {uuid}")));
        parts.push(CreateContainerComponent::ActionRow(
            CreateActionRow::buttons(vec![
                CreateButton::new(format!("{prefix}_remove_account:{target_id}:{uuid}"))
                    .label("Remove")
                    .style(ButtonStyle::Danger)
                    .disabled(!can_modify),
            ]),
        ));
        return;
    }

    let mut options = vec![
        CreateSelectMenuOption::new(name.to_string(), uuid.to_string()).default_selection(true),
    ];
    for alt in links.alts.iter().filter(|a| a.uuid != *uuid) {
        options.push(CreateSelectMenuOption::new(
            account_name(alt).to_string(),
            alt.uuid.clone(),
        ));
    }

    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("{prefix}_swap_primary:{target_id}"),
                CreateSelectMenuKind::String {
                    options: options.into(),
                },
            )
            .disabled(!can_modify),
        ),
    ));
    parts.push(text(format!("-# UUID: {uuid}")));
}

fn build_alt_sections(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    alts: &[LinkedAccount],
    can_modify: bool,
    prefix: &str,
    target_id: u64,
) {
    for alt in alts {
        parts.push(separator());
        parts.push(CreateContainerComponent::Section(CreateSection::new(
            vec![interact::section_text(&format!(
                "**`{}`**\n-# UUID: {}",
                account_name(alt),
                alt.uuid
            ))],
            CreateSectionAccessory::Button(
                CreateButton::new(format!("{prefix}_remove_account:{target_id}:{}", alt.uuid))
                    .label("Remove")
                    .style(ButtonStyle::Danger)
                    .disabled(!can_modify),
            ),
        )));
    }
}

fn build_accounts_actions(
    parts: &mut Vec<CreateContainerComponent<'static>>,
    can_modify: bool,
    prefix: &str,
    target_id: u64,
) {
    parts.push(separator());
    let buttons = vec![
        CreateButton::new(format!("{prefix}_link_new:{target_id}"))
            .label("Link New Account")
            .style(ButtonStyle::Primary)
            .disabled(!can_modify),
        CreateButton::new(format!("{prefix}_accounts_back:{target_id}"))
            .label("Back")
            .style(ButtonStyle::Secondary),
    ];
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(buttons),
    ));
}

async fn build_link_new_view(
    data: &Data,
    prefix: &str,
    target_id: u64,
) -> Vec<CreateComponent<'static>> {
    let suggested: Vec<(String, String)> = data
        .api
        .link_methods(target_id)
        .await
        .map(|methods| methods.suggested)
        .unwrap_or_default()
        .into_iter()
        .map(|account| {
            let name = account_name(&account).to_string();
            (account.uuid, name)
        })
        .collect();

    let mut parts = vec![text("### Link New Account")];
    parts.extend(crate::commands::user::link::build_link_parts(
        &suggested, prefix, target_id,
    ));
    parts.push(separator());
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(format!("{prefix}_accounts_back:{target_id}"))
                .label("Back")
                .style(ButtonStyle::Secondary),
        ]),
    ));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn refresh_view(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    can_modify: bool,
    target_id: u64,
) -> Result<()> {
    let prefix = context_prefix(&component.data.custom_id);
    interact::update_message(
        ctx,
        component,
        build_accounts_view(data, can_modify, target_id, prefix).await?,
    )
    .await
}

async fn refresh_view_from_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    can_modify: bool,
    target_id: u64,
) -> Result<()> {
    let prefix = context_prefix(&modal.data.custom_id);
    interact::update_modal(
        ctx,
        modal,
        build_accounts_view(data, can_modify, target_id, prefix).await?,
    )
    .await
}

pub async fn build_accounts_for_self(
    data: &Data,
    discord_id: u64,
) -> Result<Vec<CreateComponent<'static>>> {
    build_accounts_view(data, true, discord_id, "link").await
}

pub async fn handle_accounts_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    let prefix = context_prefix(&component.data.custom_id);
    let can_modify = resolve_can_modify(prefix, invoker_rank, target_rank, is_self);
    interact::update_message(
        ctx,
        component,
        build_accounts_view(data, can_modify, target_id, prefix).await?,
    )
    .await
}

pub async fn handle_dashboard_accounts_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = component.user.id.get();
    let components = build_accounts_view(data, true, target_id, "dashboard").await?;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_accounts_back_generic(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    match context_prefix(&component.data.custom_id) {
        "dashboard" => handle_dashboard_accounts_back(ctx, component, data).await,
        "manage" => handle_manage_accounts_back(ctx, component, data).await,
        "link" => {
            let target_id = component.user.id.get();
            let components = build_accounts_view(data, true, target_id, "link").await?;
            interact::update_message(ctx, component, components).await
        }
        _ => Ok(()),
    }
}

pub async fn handle_manage_accounts_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let (invoker_rank, _, _) = fetch_context(data, invoker_id, target_id).await?;
    interact::update_message(
        ctx,
        component,
        super::manage::build_main_view(data, invoker_rank, target_id).await,
    )
    .await
}

pub async fn handle_dashboard_accounts_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let repo = MemberRepository::new(data.db.pool());
    let Some(member) = repo
        .get_by_discord_id(component.user.id.get() as i64)
        .await?
    else {
        return interact::send_component_error(ctx, component, "Error", "You are not registered.")
            .await;
    };
    let components = crate::commands::user::dashboard::build_dashboard_view(&member, data).await;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_link_new(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    let prefix = context_prefix(&component.data.custom_id);
    let components = build_link_new_view(data, prefix, target_id).await;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_link_pick(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid select ID"))?;
    let Some(uuid) = extract_select_value(component) else {
        return Ok(());
    };

    let error = match data
        .api
        .link_account(target_id, LinkProof::Social(uuid), false)
        .await
    {
        Ok(_) => return refresh_view(ctx, component, data, true, target_id).await,
        Err(error) => error,
    };
    match error.link_refusal() {
        Some(LinkRefusal::AlreadyLinked) => send_already_linked_component(ctx, component).await,
        Some(_) => {
            interact::send_component_error(
                ctx,
                component,
                "Verification Failed",
                "That account's Discord link no longer matches.",
            )
            .await
        }
        None => Err(error.into()),
    }
}

async fn send_already_linked_component(
    ctx: &Context,
    component: &ComponentInteraction,
) -> Result<()> {
    interact::send_component_error(
        ctx,
        component,
        "Already Linked",
        "You already have this account linked.",
    )
    .await
}

async fn send_already_linked_modal(ctx: &Context, modal: &ModalInteraction) -> Result<()> {
    interact::send_modal_error(
        ctx,
        modal,
        "Already Linked",
        "You already have this account linked.",
    )
    .await
}

pub async fn handle_add_account_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    let prefix = context_prefix(&component.data.custom_id);

    let input = CreateInputText::new(InputTextStyle::Short, "username")
        .placeholder("Minecraft username")
        .min_length(1)
        .max_length(16);
    let modal = CreateModal::new(
        format!("{prefix}_add_account_modal:{target_id}"),
        "Add Account",
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Minecraft Username",
        input,
    ))]);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_add_code_button(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    let prefix = context_prefix(&component.data.custom_id);

    let input = CreateInputText::new(InputTextStyle::Short, "code")
        .placeholder("1234")
        .min_length(4)
        .max_length(4);
    let modal = CreateModal::new(
        format!("{prefix}_add_code_modal:{target_id}"),
        "Add Account via Code",
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        "Verification Code",
        input,
    ))]);

    component
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

pub async fn handle_add_code_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let prefix = context_prefix(&modal.data.custom_id);
    let target_id =
        interact::parse_id(&modal.data.custom_id).ok_or_else(|| anyhow!("Invalid modal ID"))?;
    let code = interact::extract_modal_value(&modal.data.components, "code");

    let invoker_id = modal.user.id.get();
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;
    let is_self = invoker_id == target_id;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_modal_error(ctx, modal, "Error", "Insufficient permissions").await;
    }

    let error = match data
        .api
        .link_account(target_id, LinkProof::Code(&code), false)
        .await
    {
        Ok(_) => {
            let can_modify = resolve_can_modify(prefix, invoker_rank, target_rank, is_self);
            return refresh_view_from_modal(ctx, modal, data, can_modify, target_id).await;
        }
        Err(error) => error,
    };
    match error.link_refusal() {
        Some(LinkRefusal::AlreadyLinked) => send_already_linked_modal(ctx, modal).await,
        Some(LinkRefusal::TooManyAttempts) => {
            interact::send_modal_error(
                ctx,
                modal,
                "Too Many Attempts",
                "Wait a few minutes, then enter a new code.",
            )
            .await
        }
        Some(_) => {
            interact::send_modal_error(
                ctx, modal, "Invalid Code",
                "That code is invalid or has expired.\n\nJoin the verification server to get a new code.",
            )
            .await
        }
        None => Err(error.into()),
    }
}

pub async fn handle_add_account_modal(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
) -> Result<()> {
    let prefix = context_prefix(&modal.data.custom_id);
    let target_id =
        interact::parse_id(&modal.data.custom_id).ok_or_else(|| anyhow!("Invalid modal ID"))?;
    let username = interact::extract_modal_value(&modal.data.components, "username");

    let invoker_id = modal.user.id.get();
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;
    let is_self = invoker_id == target_id;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_modal_error(ctx, modal, "Error", "Insufficient permissions").await;
    }

    let error = match data
        .api
        .link_account(target_id, LinkProof::Social(&username), false)
        .await
    {
        Ok(_) => {
            let can_modify = resolve_can_modify(prefix, invoker_rank, target_rank, is_self);
            return refresh_view_from_modal(ctx, modal, data, can_modify, target_id).await;
        }
        Err(error) => error,
    };
    let refusal = match error.link_refusal() {
        Some(LinkRefusal::AlreadyLinked) => return send_already_linked_modal(ctx, modal).await,
        Some(refusal) => refusal,
        None => return Err(error.into()),
    };

    if is_self && invoker_rank < AccessRank::Moderator {
        return interact::send_modal_error(
            ctx,
            modal,
            "Error",
            &unverified_message(refusal, &username),
        )
        .await;
    }

    match data.api.resolve(&username).await {
        Ok(player) => {
            show_force_link_prompt(
                ctx,
                modal,
                data,
                &player.username,
                prefix,
                target_id,
                &player.uuid,
            )
            .await
        }
        Err(_) => {
            interact::send_modal_error(
                ctx,
                modal,
                "Error",
                &format!("Could not find player: {username}"),
            )
            .await
        }
    }
}

fn unverified_message(refusal: LinkRefusal, username: &str) -> String {
    match refusal {
        LinkRefusal::NotFound => format!("Could not find player: {username}"),
        LinkRefusal::Unconfirmed => "Your Discord must be linked in Hypixel social settings for this account\n\n-# *Hypixel was unreachable — using cached data. If you just updated your link, try again shortly.*".into(),
        _ => "Your Discord must be linked in Hypixel social settings for this account".into(),
    }
}

async fn show_force_link_prompt(
    ctx: &Context,
    modal: &ModalInteraction,
    data: &Data,
    player_name: &str,
    prefix: &str,
    target_id: u64,
    uuid: &str,
) -> Result<()> {
    let mut components = build_accounts_view(data, true, target_id, prefix).await?;
    components.push(CreateComponent::Container(
        CreateContainer::new(vec![
            text(format!(
                "**{player_name}** does not have <@{target_id}>'s Discord linked in Hypixel social settings.",
            )),
            separator(),
            CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
                CreateButton::new(format!("{prefix}_force_add:{target_id}:{uuid}"))
                    .label("Force Link").style(ButtonStyle::Danger),
                CreateButton::new(format!("{prefix}_cancel_add:{target_id}"))
                    .label("Cancel").style(ButtonStyle::Secondary),
            ])),
        ])
        .accent_color(channel::COLOR_ERROR),
    ));
    interact::update_modal(ctx, modal, components).await
}

pub async fn handle_force_add(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (target_id, uuid) = interact::parse_ids(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    match data
        .api
        .link_account(target_id, LinkProof::Force(&uuid), false)
        .await
    {
        Ok(_) => refresh_view(ctx, component, data, true, target_id).await,
        Err(error) if error.link_refusal() == Some(LinkRefusal::AlreadyLinked) => {
            send_already_linked_component(ctx, component).await
        }
        Err(error) => Err(error.into()),
    }
}

pub async fn handle_remove_account(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (target_id, uuid) = interact::parse_ids(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    match data.api.unlink_account(target_id, &uuid).await {
        Ok(_) | Err(ApiError::NotFound) => {}
        Err(error) => return Err(error.into()),
    }

    let prefix = context_prefix(&component.data.custom_id);
    let can_modify = resolve_can_modify(prefix, invoker_rank, target_rank, is_self);
    refresh_view(ctx, component, data, can_modify, target_id).await
}

pub async fn handle_swap_primary(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid select ID"))?;
    let Some(new_uuid) = extract_select_value(component) else {
        return Ok(());
    };

    let invoker_id = component.user.id.get();
    let is_self = invoker_id == target_id;
    let (invoker_rank, _, target_rank) = fetch_context(data, invoker_id, target_id).await?;

    if !is_self && (invoker_rank < AccessRank::Moderator || invoker_rank <= target_rank) {
        return interact::send_component_error(ctx, component, "Error", "Insufficient permissions")
            .await;
    }

    match data.api.set_primary(target_id, new_uuid).await {
        Ok(_) | Err(ApiError::NotFound) => {}
        Err(error) => return Err(error.into()),
    }

    let prefix = context_prefix(&component.data.custom_id);
    let can_modify = resolve_can_modify(prefix, invoker_rank, target_rank, is_self);
    refresh_view(ctx, component, data, can_modify, target_id).await
}

pub async fn handle_cancel_add(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id = interact::parse_id(&component.data.custom_id)
        .ok_or_else(|| anyhow!("Invalid button ID"))?;
    refresh_view(ctx, component, data, true, target_id).await
}
