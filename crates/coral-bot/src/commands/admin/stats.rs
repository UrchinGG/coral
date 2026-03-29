use anyhow::Result;
use serenity::all::*;

use database::{BlacklistRepository, CacheRepository, MemberRepository};

use crate::{
    framework::Data,
    utils::{format_number, separator, text},
};


pub fn register() -> CreateCommand<'static> {
    CreateCommand::new("stats").description("View database statistics")
}


pub async fn run(ctx: &Context, command: &CommandInteraction, data: &Data) -> Result<()> {
    command.defer(&ctx.http).await?;

    let pool = data.db.pool();
    let member_repo = MemberRepository::new(pool);
    let blacklist_repo = BlacklistRepository::new(pool);
    let cache_repo = CacheRepository::new(pool);

    let (members, requests, players, tags, tag_breakdown, snapshots, tracked) = tokio::join!(
        member_repo.count(),
        member_repo.total_requests(),
        blacklist_repo.count_players(),
        blacklist_repo.count_active_tags(),
        blacklist_repo.count_tags_by_type(),
        cache_repo.count_snapshots(),
        cache_repo.count_unique_players(),
    );

    let (members, requests, players, tags) = (
        members.unwrap_or(0), requests.unwrap_or(0),
        players.unwrap_or(0), tags.unwrap_or(0),
    );
    let (snapshots, tracked) = (snapshots.unwrap_or(0), tracked.unwrap_or(0));
    let tag_breakdown = tag_breakdown.unwrap_or_default();

    let tag_lines: Vec<String> = tag_breakdown.iter().map(|(tag_type, count)| {
        let emote = blacklist::lookup(tag_type).map(|d| d.emote).unwrap_or("");
        format!("{emote} **{}** {}", format_number(*count as u64), tag_type.replace('_', " "))
    }).collect();
    let tag_display = if tag_lines.is_empty() { "No tags yet".into() } else { tag_lines.join("\n") };

    let mut parts: Vec<CreateContainerComponent> = vec![text("## Database Statistics")];
    parts.push(separator());
    parts.push(text(format!(
        "### Members\n\
         **{}** registered · **{}** lifetime requests",
        format_number(members as u64),
        format_number(requests as u64),
    )));
    parts.push(separator());
    parts.push(text(format!(
        "### Blacklist\n\
         **{}** players · **{}** active tags\n{tag_display}",
        format_number(players as u64),
        format_number(tags as u64),
    )));
    parts.push(separator());
    parts.push(text(format!(
        "### Cache\n\
         **{}** snapshots · **{}** players tracked",
        format_number(snapshots as u64),
        format_number(tracked as u64),
    )));

    command.edit_response(&ctx.http, EditInteractionResponse::new()
        .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
        .components(vec![CreateComponent::Container(CreateContainer::new(parts))]),
    ).await?;
    Ok(())
}
