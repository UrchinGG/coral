use anyhow::Result;
use serenity::all::*;

use blacklist::lookup as lookup_tag;
use clients::is_uuid;
use database::{BlacklistRepository, CacheRepository, PlayerEvent};

use crate::{
    framework::Data,
    utils::{format_number, separator, text},
};

const PER_PAGE: usize = 10;

async fn fetch_guild(data: &Data, query: &str) -> Option<hypixel::Guild> {
    hypixel::Guild::from_value(&resolve_guild(data, query).await?)
}

async fn resolve_guild(data: &Data, query: &str) -> Option<serde_json::Value> {
    if is_uuid(query) {
        return data.api.get_guild_by_player(query).await.ok().flatten();
    }
    if query.len() <= 16 {
        if let Ok(resolved) = data.api.resolve(query).await {
            if let Some(guild) = data
                .api
                .get_guild_by_player(&resolved.uuid)
                .await
                .ok()
                .flatten()
            {
                return Some(guild);
            }
        }
    }
    data.api.get_guild_by_name(query).await.ok().flatten()
}

pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("guild")
        .description("View blacklisted players in a Hypixel guild")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::String,
                "query",
                "Guild name, or a player in the guild",
            )
            .required(true),
        )
}

pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;
    let query = command
        .data
        .options()
        .into_iter()
        .find(|o| o.name == "query")
        .and_then(|o| match o.value {
            ResolvedValue::String(s) => Some(s.to_string()),
            _ => None,
        })
        .unwrap_or_default();

    let components = build_view(data, &query, 0)
        .await
        .unwrap_or_else(|| not_found(&query));
    command
        .edit_response(&ctx.http, page_response(components))
        .await?;
    Ok(())
}

pub async fn handle_page(
    ctx: &Context,
    component: &ComponentInteraction,
    data: &Data,
) -> Result<()> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    let (page, query) = parse_action(&component.data.custom_id, "guild_pg:");
    let components = build_view(data, query, page)
        .await
        .unwrap_or_else(|| not_found(query));
    component
        .edit_response(&ctx.http, page_response(components))
        .await?;
    Ok(())
}

async fn build_view(
    data: &Data,
    query: &str,
    page: usize,
) -> Option<Vec<CreateComponent<'static>>> {
    let guild = fetch_guild(data, query).await?;
    let pool = data.db.pool();

    let uuids: Vec<String> = guild.members.iter().map(|m| m.uuid.clone()).collect();
    let mut tagged: Vec<(String, Vec<PlayerEvent>)> = BlacklistRepository::new(pool)
        .get_players_batch(&uuids)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, events)| !events.is_empty())
        .collect();
    tagged.sort_by_key(|(_, events)| top_priority(events));

    let names = CacheRepository::new(pool)
        .usernames(
            &tagged
                .iter()
                .map(|(uuid, _)| uuid.clone())
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap_or_default();

    let total = tagged.len();
    let pages = total.div_ceil(PER_PAGE).max(1);
    let page = page.min(pages - 1);

    let mut parts: Vec<CreateContainerComponent> = vec![
        text(format!("## {}", guild.name)),
        text(format!(
            "-# **{}** members | **{}** blacklisted",
            format_number(guild.members.len() as u64),
            format_number(total as u64),
        )),
        separator(),
    ];

    if tagged.is_empty() {
        parts.push(text("No blacklisted players in this guild."));
    } else {
        let lines: Vec<String> = tagged
            .iter()
            .skip(page * PER_PAGE)
            .take(PER_PAGE)
            .map(|(uuid, events)| member_line(uuid, events, &names))
            .collect();
        parts.push(text(lines.join("\n")));
    }

    if pages > 1 {
        parts.push(text(format!("-# Page {} of {pages}", page + 1)));
        parts.push(page_buttons(query, page, pages));
    }

    Some(vec![CreateComponent::Container(CreateContainer::new(
        parts,
    ))])
}

fn member_line(
    uuid: &str,
    events: &[PlayerEvent],
    names: &std::collections::HashMap<String, String>,
) -> String {
    let name = names
        .get(uuid)
        .cloned()
        .unwrap_or_else(|| uuid[..uuid.len().min(8)].to_string());
    let tag = events
        .iter()
        .filter_map(|e| e.tag_type.as_deref())
        .min_by_key(|t| lookup_tag(t).map(|d| d.priority).unwrap_or(u8::MAX));
    match tag {
        Some(tag) => {
            let def = lookup_tag(tag);
            let emote = def.map(|d| d.emote).unwrap_or("");
            let display = def.map(|d| d.display_name).unwrap_or(tag);
            format!("{emote} `{name}` **{display}**")
        }
        None => format!("`{name}`"),
    }
}

fn top_priority(events: &[PlayerEvent]) -> u8 {
    events
        .iter()
        .filter_map(|e| e.tag_type.as_deref())
        .filter_map(|t| lookup_tag(t).map(|d| d.priority))
        .min()
        .unwrap_or(u8::MAX)
}

fn page_buttons(query: &str, page: usize, pages: usize) -> CreateContainerComponent<'static> {
    let prev = CreateButton::new(format!("guild_pg:{}:{query}", page.saturating_sub(1)))
        .label("Prev")
        .style(ButtonStyle::Secondary)
        .disabled(page == 0);
    let next = CreateButton::new(format!("guild_pg:{}:{query}", page + 1))
        .label("Next")
        .style(ButtonStyle::Secondary)
        .disabled(page + 1 >= pages);
    CreateContainerComponent::ActionRow(CreateActionRow::Buttons(vec![prev, next].into()))
}

fn page_response(components: Vec<CreateComponent<'static>>) -> EditInteractionResponse<'static> {
    EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .components(components)
}

fn parse_action<'a>(custom_id: &'a str, prefix: &str) -> (usize, &'a str) {
    let rest = custom_id.strip_prefix(prefix).unwrap_or("");
    let (page, query) = rest.split_once(':').unwrap_or(("0", ""));
    (page.parse().unwrap_or(0), query)
}

fn not_found(query: &str) -> Vec<CreateComponent<'static>> {
    vec![CreateComponent::Container(CreateContainer::new(vec![
        text(format!("No guild found for `{query}`.")),
    ]))]
}
