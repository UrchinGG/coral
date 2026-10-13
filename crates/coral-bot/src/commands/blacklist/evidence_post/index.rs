use std::collections::HashSet;

use serenity::all::*;

use database::{BlacklistRepository, EvidencePostRepository, NewPost, Origin, PostStatus};

use super::media::{current_media, gallery_files};
use super::ops;
use crate::framework::Data;

const BASE_TAGS: &[&str] = &["closet_cheater", "blatant_cheater"];

pub async fn reconcile(ctx: &Context, data: &Data) {
    let Some(threads) = list_forum_threads(ctx, data).await else {
        return;
    };
    let repo = EvidencePostRepository::new(data.db.pool());
    let known: HashSet<i64> = match repo.known_thread_ids().await {
        Ok(ids) => ids.into_iter().collect(),
        Err(e) => {
            tracing::error!("Evidence index reconcile could not read rows: {e}");
            return;
        }
    };
    let bot_id = ctx.cache.current_user().id;
    let mut added = 0;
    for thread in &threads {
        if known.contains(&(thread.id.get() as i64)) {
            continue;
        }
        if insert_discovered(ctx, data, thread, bot_id).await {
            added += 1;
        }
    }
    let present: Vec<i64> = threads.iter().map(|t| t.id.get() as i64).collect();
    let removed = repo.delete_missing(&present).await.unwrap_or_else(|e| {
        tracing::error!("Evidence index reconcile could not prune rows: {e}");
        0
    });
    tracing::info!(
        "evidence index: {} threads, {added} added, {removed} removed",
        threads.len()
    );
    archive_orphans(ctx, data).await;
}

pub async fn archive_orphans(ctx: &Context, data: &Data) {
    let orphans = match EvidencePostRepository::new(data.db.pool())
        .confirmed_without_tag()
        .await
    {
        Ok(orphans) => orphans,
        Err(e) => {
            tracing::error!("Evidence archive reconcile failed to read rows: {e}");
            return;
        }
    };
    for post in orphans {
        if let Err(e) = ops::archive(ctx, data, post.thread_id).await {
            tracing::error!("Archiving evidence post {} failed: {e}", post.thread_id);
        }
    }
}

pub async fn on_thread_create(ctx: &Context, data: &Data, thread: &GuildThread) {
    if Some(thread.parent_id) != data.evidence_forum_id {
        return;
    }
    if thread.owner_id == ctx.cache.current_user().id {
        return;
    }
    let Some(uuid) = uuid_from_title(&thread.base.name) else {
        return;
    };
    if let Err(e) = EvidencePostRepository::new(data.db.pool())
        .insert(NewPost {
            thread_id: thread.id.get() as i64,
            uuid: &uuid,
            origin: Origin::Legacy,
            status: None,
            title: &thread.base.name,
            created_at: *thread.id.created_at(),
        })
        .await
    {
        tracing::error!("Failed to index evidence thread {}: {e}", thread.id);
    }
}

pub async fn on_thread_delete(data: &Data, thread_id: ThreadId) {
    if let Err(e) = EvidencePostRepository::new(data.db.pool())
        .delete(thread_id.get() as i64)
        .await
    {
        tracing::error!("Failed to unindex evidence thread {thread_id}: {e}");
    }
}

async fn insert_discovered(
    ctx: &Context,
    data: &Data,
    thread: &GuildThread,
    bot_id: UserId,
) -> bool {
    let coral = thread.owner_id == bot_id;
    let uuid = match uuid_from_title(&thread.base.name) {
        Some(uuid) => Some(uuid),
        None if coral => uuid_from_footer(ctx, thread.id).await,
        None => None,
    };
    let Some(uuid) = uuid else {
        tracing::warn!(
            "Evidence thread {} has no readable uuid; skipped",
            thread.id
        );
        return false;
    };
    let status = if coral {
        Some(classify(ctx, data, thread, &uuid).await)
    } else {
        None
    };
    EvidencePostRepository::new(data.db.pool())
        .insert(NewPost {
            thread_id: thread.id.get() as i64,
            uuid: &uuid,
            origin: if coral { Origin::Coral } else { Origin::Legacy },
            status,
            title: &thread.base.name,
            created_at: *thread.id.created_at(),
        })
        .await
        .inspect_err(|e| tracing::error!("Failed to index evidence thread {}: {e}", thread.id))
        .unwrap_or(false)
}

async fn classify(ctx: &Context, data: &Data, thread: &GuildThread, uuid: &str) -> PostStatus {
    if thread.thread_metadata.locked() || renders_archived(ctx, thread.id).await {
        return PostStatus::Archived;
    }
    let tags = BlacklistRepository::new(data.db.pool())
        .get_active_tags(uuid)
        .await
        .unwrap_or_default();
    let holds = |wanted: &[&str]| {
        tags.iter()
            .any(|t| t.tag_type.as_deref().is_some_and(|ty| wanted.contains(&ty)))
    };
    if holds(&["confirmed_cheater"]) {
        return PostStatus::Confirmed;
    }
    let empty = current_media(ctx, thread.id.get() as i64)
        .await
        .map(|media| gallery_files(&media).is_empty())
        .unwrap_or(true);
    if holds(BASE_TAGS) && empty {
        PostStatus::Pending
    } else {
        PostStatus::Confirmed
    }
}

async fn renders_archived(ctx: &Context, thread_id: ThreadId) -> bool {
    first_message_text(ctx, thread_id)
        .await
        .is_some_and(|text| text.contains("` (Archived)"))
}

async fn uuid_from_footer(ctx: &Context, thread_id: ThreadId) -> Option<String> {
    let text = first_message_text(ctx, thread_id).await?;
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("-# UUID:"))?;
    uuid_from_title(line)
}

async fn first_message_text(ctx: &Context, thread_id: ThreadId) -> Option<String> {
    let message = ctx
        .http
        .get_message(thread_id.into(), MessageId::new(thread_id.get()))
        .await
        .ok()?;
    let mut text = Vec::new();
    collect_text(&message.components, &mut text);
    Some(text.join("\n"))
}

fn collect_text(components: &[Component], out: &mut Vec<String>) {
    for component in components {
        match component {
            Component::Container(container) => {
                for part in container.components.iter() {
                    match part {
                        ContainerComponent::TextDisplay(display) => {
                            out.push(display.content.clone().unwrap_or_default().to_string())
                        }
                        ContainerComponent::Section(section) => {
                            for inner in section.components.iter() {
                                if let SectionComponent::TextDisplay(display) = inner {
                                    out.push(
                                        display.content.clone().unwrap_or_default().to_string(),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Component::TextDisplay(display) => {
                out.push(display.content.clone().unwrap_or_default().to_string())
            }
            _ => {}
        }
    }
}

pub fn uuid_from_title(title: &str) -> Option<String> {
    title
        .split(|c: char| !c.is_ascii_hexdigit() && c != '-')
        .map(|token| token.replace('-', ""))
        .find(|uuid| uuid.len() == 32)
        .map(|uuid| uuid.to_ascii_lowercase())
}

async fn list_forum_threads(ctx: &Context, data: &Data) -> Option<Vec<GuildThread>> {
    let forum_id = data.evidence_forum_id?;
    let guild_id = data.home_guild_id?;
    let mut threads: Vec<GuildThread> = match ctx.http.get_guild_active_threads(guild_id).await {
        Ok(active) => active
            .threads
            .into_iter()
            .filter(|t| t.parent_id == forum_id)
            .collect(),
        Err(e) => {
            tracing::warn!(
                "Evidence index: could not list active threads, skipping reconcile: {e}"
            );
            return None;
        }
    };
    let mut before: Option<Timestamp> = None;
    loop {
        match ctx
            .http
            .get_channel_archived_public_threads(forum_id, before, Some(100))
            .await
        {
            Ok(batch) => {
                let next = batch
                    .threads
                    .last()
                    .and_then(|t| t.thread_metadata.archive_timestamp);
                let has_more = batch.has_more;
                threads.extend(batch.threads);
                match next {
                    Some(next) if has_more => before = Some(next),
                    _ => return Some(threads),
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Evidence index: could not page archived threads, skipping reconcile: {e}"
                );
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dashed_uuid_from_title_or_footer() {
        assert_eq!(
            uuid_from_title("Hexze | 981d4c2b-5a0f-479a-9d33-8e614a2d8aa2").as_deref(),
            Some("981d4c2b5a0f479a9d338e614a2d8aa2")
        );
        assert_eq!(
            uuid_from_title("-# UUID: 981d4c2b-5a0f-479a-9d33-8e614a2d8aa2").as_deref(),
            Some("981d4c2b5a0f479a9d338e614a2d8aa2")
        );
        assert!(uuid_from_title("CatchYourBreath! | LEGACY").is_none());
    }
}
