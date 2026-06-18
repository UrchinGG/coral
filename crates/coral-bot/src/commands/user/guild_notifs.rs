use anyhow::Result;
use serenity::all::*;

use database::{
    AccountRepository, CacheRepository, GuildCurrentRepository, GuildSubscriptionRepository,
    Member, MemberRepository,
};

use crate::framework::Data;
use crate::interact;
use crate::utils::{separator, text};

async fn member_of(component: &ComponentInteraction, data: &Data) -> Option<Member> {
    MemberRepository::new(data.db.pool())
        .get_by_discord_id(component.user.id.get() as i64)
        .await
        .ok()
        .flatten()
}

pub(crate) async fn build_view(member: &Member, data: &Data) -> Vec<CreateComponent<'static>> {
    let pool = data.db.pool();
    let sub = GuildSubscriptionRepository::new(pool)
        .get_for_user(member.discord_id)
        .await
        .ok()
        .flatten();

    let mut parts: Vec<CreateContainerComponent> = vec![text("## Guild Notifications")];

    match &sub {
        Some(sub) => {
            let guild_name = GuildCurrentRepository::new(pool)
                .get(&sub.guild_id)
                .await
                .ok()
                .flatten()
                .and_then(|(raw, _)| raw["name"].as_str().map(String::from))
                .unwrap_or_else(|| sub.guild_id.clone());
            let alerts = if sub.tag_types.is_empty() {
                "all tags".to_string()
            } else {
                sub.tag_types
                    .iter()
                    .filter_map(|t| blacklist::lookup(t).map(|d| d.display_name))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            parts.push(text(format!("Watching **{guild_name}**")));
            parts.push(text(format!("-# Alerts for: {alerts}")));
            parts.push(separator());
            parts.push(tag_select(&sub.tag_types));
        }
        None => {
            parts.push(text("You're not watching a guild."));
            parts.push(separator());
        }
    }

    match account_select(member, data).await {
        Some(menu) => parts.push(menu),
        None => parts.push(text("You have no linked accounts to watch a guild with.")),
    }

    let mut buttons = Vec::new();
    if sub.is_some() {
        buttons.push(
            CreateButton::new("gn_stop")
                .label("Stop Watching")
                .style(ButtonStyle::Danger),
        );
    }
    buttons.push(
        CreateButton::new("gn_back")
            .label("Back")
            .style(ButtonStyle::Secondary),
    );
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::Buttons(buttons.into()),
    ));

    vec![CreateComponent::Container(CreateContainer::new(parts))]
}

async fn account_select(member: &Member, data: &Data) -> Option<CreateContainerComponent<'static>> {
    let pool = data.db.pool();
    let mut uuids: Vec<String> = member.uuid.iter().cloned().collect();
    for account in AccountRepository::new(pool)
        .list(member.id)
        .await
        .unwrap_or_default()
    {
        if !uuids.contains(&account.uuid) {
            uuids.push(account.uuid);
        }
    }
    if uuids.is_empty() {
        return None;
    }

    let names = CacheRepository::new(pool)
        .usernames(&uuids)
        .await
        .unwrap_or_default();
    let options: Vec<CreateSelectMenuOption> = uuids
        .iter()
        .map(|uuid| {
            let label = names.get(uuid).cloned().unwrap_or_else(|| uuid.clone());
            CreateSelectMenuOption::new(label, uuid.clone())
        })
        .collect();

    Some(CreateContainerComponent::ActionRow(
        CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                "gn_account",
                CreateSelectMenuKind::String {
                    options: options.into(),
                },
            )
            .placeholder("Choose Account"),
        ),
    ))
}

fn tag_select(current: &[String]) -> CreateContainerComponent<'static> {
    let options: Vec<CreateSelectMenuOption> = blacklist::all()
        .iter()
        .map(|def| {
            let mut option = CreateSelectMenuOption::new(def.display_name, def.name);
            if current.is_empty() || current.iter().any(|t| t == def.name) {
                option = option.default_selection(true);
            }
            option
        })
        .collect();
    let count = options.len();
    CreateContainerComponent::ActionRow(CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            "gn_tags",
            CreateSelectMenuKind::String {
                options: options.into(),
            },
        )
        .min_values(0)
        .max_values(count as u8)
        .placeholder("Choose Tags"),
    ))
}

pub async fn handle_open(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let Some(member) = member_of(component, data).await else {
        return Ok(());
    };
    let components = build_view(&member, data).await;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_account(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let ComponentInteractionDataKind::StringSelect { values } = &component.data.kind else {
        return Ok(());
    };
    let Some(uuid) = values.first() else {
        return Ok(());
    };

    let Some(raw) = data.api.get_guild_by_player(uuid).await.ok().flatten() else {
        return interact::send_component_error(
            ctx,
            component,
            "Not in a guild",
            "That account isn't in a guild.",
        )
        .await;
    };
    if let Some(guild_id) = raw["_id"].as_str() {
        let _ = GuildSubscriptionRepository::new(data.db.pool())
            .set_for_user(component.user.id.get() as i64, guild_id, &[])
            .await;
    }

    let Some(member) = member_of(component, data).await else {
        return Ok(());
    };
    let components = build_view(&member, data).await;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_tags(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let selected: Vec<String> = match &component.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.iter().cloned().collect(),
        _ => Vec::new(),
    };
    let _ = GuildSubscriptionRepository::new(data.db.pool())
        .set_tags(component.user.id.get() as i64, &selected)
        .await;

    let Some(member) = member_of(component, data).await else {
        return Ok(());
    };
    let components = build_view(&member, data).await;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_stop(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let _ = GuildSubscriptionRepository::new(data.db.pool())
        .clear_for_user(component.user.id.get() as i64)
        .await;

    let Some(member) = member_of(component, data).await else {
        return Ok(());
    };
    let components = build_view(&member, data).await;
    interact::update_message(ctx, component, components).await
}

pub async fn handle_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let Some(member) = member_of(component, data).await else {
        return Ok(());
    };
    let components = super::dashboard::build_dashboard_view(&member, data).await;
    interact::update_message(ctx, component, components).await
}
