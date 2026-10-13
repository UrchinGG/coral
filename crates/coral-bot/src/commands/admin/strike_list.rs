use anyhow::{Result, anyhow};
use chrono::Utc;
use serenity::all::*;

use database::{Strike, StrikeRepository, StrikeStatus};

use crate::{
    framework::{AccessRank, Data},
    interact,
    utils::{sanitize_reason, separator, text},
};

use super::manage::{build_main_view, fetch_context, require_mod_over};

const ACTIVE_PAGE_SIZE: usize = 3;
const PAST_PAGE_SIZE: usize = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Pages {
    active: usize,
    past: usize,
}

pub fn summary_section(target_id: u64, strikes: &[Strike]) -> CreateContainerComponent<'static> {
    if strikes.is_empty() {
        return text("### Strikes\n-# No strike history");
    }
    let active = strikes.iter().filter(|s| s.is_active()).count();
    let past = strikes.len() - active;
    CreateContainerComponent::Section(CreateSection::new(
        vec![interact::section_text(&format!(
            "### Strikes\n-# {active} active, {past} past"
        ))],
        CreateSectionAccessory::Button(
            CreateButton::new(list_id(target_id, Pages::default()))
                .label("View")
                .style(ButtonStyle::Secondary),
        ),
    ))
}

pub async fn handle_list(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (target_id, rest) =
        interact::parse_ids(&component.data.custom_id).ok_or_else(|| anyhow!("Invalid ID"))?;
    let Some((invoker_rank, target_rank)) = staff_ranks(ctx, component, data, target_id).await?
    else {
        return Ok(());
    };
    let view = list_view(
        data,
        invoker_rank,
        target_rank,
        target_id,
        parse_pages(&rest),
    )
    .await?;
    interact::update_message(ctx, component, view).await
}

pub async fn handle_back(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let target_id =
        interact::parse_id(&component.data.custom_id).ok_or_else(|| anyhow!("Invalid ID"))?;
    let Some((invoker_rank, _)) = staff_ranks(ctx, component, data, target_id).await? else {
        return Ok(());
    };
    interact::update_message(
        ctx,
        component,
        build_main_view(data, invoker_rank, target_id).await,
    )
    .await
}

pub async fn handle_revoke_prompt(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (target_id, strike_id) = parse_strike_target(&component.data.custom_id)?;
    let Some(strike) = revocable(ctx, component, data, target_id, strike_id).await? else {
        return Ok(());
    };
    let view = vec![CreateComponent::Container(CreateContainer::new(vec![
        text(format!(
            "### Revoke this strike for <@{target_id}>?\n{}",
            active_strike_text(&strike)
        )),
        CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![
            CreateButton::new(format!("manage_strike_revoke_go:{target_id}:{strike_id}"))
                .label("Revoke")
                .style(ButtonStyle::Danger),
            CreateButton::new(list_id(target_id, Pages::default()))
                .label("Cancel")
                .style(ButtonStyle::Secondary),
        ])),
    ]))];
    interact::update_message(ctx, component, view).await
}

pub async fn handle_revoke_confirm(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    let (target_id, strike_id) = parse_strike_target(&component.data.custom_id)?;
    if revocable(ctx, component, data, target_id, strike_id)
        .await?
        .is_none()
    {
        return Ok(());
    }
    crate::strikes::revoke(ctx, data, strike_id, component.user.id.get()).await?;
    let (invoker_rank, _, target_rank) =
        fetch_context(data, component.user.id.get(), target_id).await?;
    let view = list_view(data, invoker_rank, target_rank, target_id, Pages::default()).await?;
    interact::update_message(ctx, component, view).await
}

async fn staff_ranks(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    target_id: u64,
) -> Result<Option<(AccessRank, AccessRank)>> {
    let (invoker_rank, _, target_rank) =
        fetch_context(data, component.user.id.get(), target_id).await?;
    if invoker_rank < AccessRank::Moderator {
        interact::send_component_error(ctx, component, "Error", "Insufficient permissions").await?;
        return Ok(None);
    }
    Ok(Some((invoker_rank, target_rank)))
}

async fn revocable(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
    target_id: u64,
    strike_id: i64,
) -> Result<Option<Strike>> {
    let (invoker_rank, _, target_rank) =
        fetch_context(data, component.user.id.get(), target_id).await?;
    if !require_mod_over(invoker_rank, target_rank) {
        interact::send_component_error(ctx, component, "Error", "Insufficient permissions").await?;
        return Ok(None);
    }
    let strike = StrikeRepository::new(data.db.pool())
        .get(strike_id)
        .await?
        .filter(|s| s.discord_id as u64 == target_id && s.is_active());
    if strike.is_none() {
        let view = list_view(data, invoker_rank, target_rank, target_id, Pages::default()).await?;
        interact::update_message(ctx, component, view).await?;
    }
    Ok(strike)
}

async fn list_view(
    data: &Data,
    invoker_rank: AccessRank,
    target_rank: AccessRank,
    target_id: u64,
    pages: Pages,
) -> Result<Vec<CreateComponent<'static>>> {
    let can_revoke = require_mod_over(invoker_rank, target_rank);
    let strikes = StrikeRepository::new(data.db.pool())
        .list_for_member(target_id as i64)
        .await?;
    let (active, past): (Vec<Strike>, Vec<Strike>) =
        strikes.into_iter().partition(Strike::is_active);

    let mut parts = vec![text(format!("### Strikes for <@{target_id}>"))];
    if active.is_empty() {
        parts.push(text("-# No active strikes"));
    }
    for strike in page_of(&active, pages.active, ACTIVE_PAGE_SIZE) {
        parts.push(CreateContainerComponent::Section(CreateSection::new(
            vec![interact::section_text(&active_strike_text(strike))],
            CreateSectionAccessory::Button(
                CreateButton::new(format!("manage_strike_revoke:{target_id}:{}", strike.id))
                    .label("Revoke")
                    .style(ButtonStyle::Danger)
                    .disabled(!can_revoke),
            ),
        )));
    }
    if active.len() > ACTIVE_PAGE_SIZE {
        parts.push(page_row(target_id, pages, active.len(), ListKind::Active));
    }

    if !past.is_empty() {
        parts.push(separator());
        let items: Vec<String> = page_of(&past, pages.past, PAST_PAGE_SIZE)
            .iter()
            .map(past_strike_text)
            .collect();
        parts.push(text(format!("### Past Strikes\n{}", items.join("\n"))));
        if past.len() > PAST_PAGE_SIZE {
            parts.push(page_row(target_id, pages, past.len(), ListKind::Past));
        }
    }

    parts.push(separator());
    parts.push(CreateContainerComponent::ActionRow(
        CreateActionRow::buttons(vec![
            CreateButton::new(format!("manage_strikes_back:{target_id}"))
                .label("Back")
                .style(ButtonStyle::Secondary),
        ]),
    ));
    Ok(vec![CreateComponent::Container(CreateContainer::new(
        parts,
    ))])
}

#[derive(Clone, Copy)]
enum ListKind {
    Active,
    Past,
}

fn page_row(
    target_id: u64,
    pages: Pages,
    len: usize,
    kind: ListKind,
) -> CreateContainerComponent<'static> {
    let (page, size) = match kind {
        ListKind::Active => (pages.active, ACTIVE_PAGE_SIZE),
        ListKind::Past => (pages.past, PAST_PAGE_SIZE),
    };
    let page_count = len.div_ceil(size);
    let turn = |to: usize| match kind {
        ListKind::Active => Pages {
            active: to,
            ..pages
        },
        ListKind::Past => Pages { past: to, ..pages },
    };
    let prev = CreateButton::new(list_id(target_id, turn(page.saturating_sub(1))))
        .label("Prev")
        .style(ButtonStyle::Secondary)
        .disabled(page == 0);
    let next = CreateButton::new(list_id(target_id, turn(page + 1)))
        .label("Next")
        .style(ButtonStyle::Secondary)
        .disabled(page + 1 >= page_count);
    CreateContainerComponent::ActionRow(CreateActionRow::buttons(vec![prev, next]))
}

fn page_of<T>(items: &[T], page: usize, size: usize) -> &[T] {
    let start = (page * size).min(items.len());
    let end = (start + size).min(items.len());
    &items[start..end]
}

fn active_strike_text(strike: &Strike) -> String {
    let expiry = match strike.expires_at {
        Some(at) => format!("expires <t:{}:R>", at.timestamp()),
        None => "no expiration".to_string(),
    };
    format!(
        "> {}\n> -# **\\-** Issued by <@{}> <t:{}:R>, {expiry}",
        sanitize_reason(&strike.reason),
        strike.struck_by,
        strike.issued_at.timestamp()
    )
}

fn past_strike_text(strike: &Strike) -> String {
    let ended = match strike.status_at(Utc::now()) {
        StrikeStatus::Revoked => strike.revoked_at.map(|at| ("Revoked", at)),
        _ => strike.expires_at.map(|at| ("Expired", at)),
    };
    let ended = ended
        .map(|(word, at)| format!(" {word} <t:{}:D>.", at.timestamp()))
        .unwrap_or_default();
    format!(
        "> {}\n-# Issued by <@{}> <t:{}:D>.{ended}",
        sanitize_reason(&strike.reason),
        strike.struck_by,
        strike.issued_at.timestamp()
    )
}

fn list_id(target_id: u64, pages: Pages) -> String {
    format!("manage_strikes:{target_id}:{}:{}", pages.active, pages.past)
}

fn parse_pages(rest: &str) -> Pages {
    let mut parts = rest.split(':').map(|p| p.parse().unwrap_or(0));
    Pages {
        active: parts.next().unwrap_or(0),
        past: parts.next().unwrap_or(0),
    }
}

fn parse_strike_target(custom_id: &str) -> Result<(u64, i64)> {
    let (target_id, rest) = interact::parse_ids(custom_id).ok_or_else(|| anyhow!("Invalid ID"))?;
    Ok((target_id, rest.parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_round_trip_through_the_custom_id() {
        let pages = Pages { active: 2, past: 1 };
        let id = list_id(42, pages);
        let (target, rest) = interact::parse_ids(&id).unwrap();
        assert_eq!(target, 42);
        assert_eq!(parse_pages(&rest), pages);
    }

    #[test]
    fn page_of_clamps_past_the_end() {
        let items = [1, 2, 3, 4];
        assert_eq!(page_of(&items, 1, 3), &[4]);
        assert!(page_of(&items, 5, 3).is_empty());
    }
}
