use serenity::all::*;

use database::CacheRepository;

use crate::framework::Data;

mod minecraft;
pub mod standing;

pub use minecraft::{
    format_number, format_tag_detail, format_uuid_dashed, generate_api_key, sanitize_reason,
    unsanitize_reason,
};

pub async fn resolve_username(uuid: &str, data: &Data) -> Option<String> {
    CacheRepository::new(data.db.pool())
        .get_username(uuid)
        .await
        .ok()
        .flatten()
}

pub async fn resolve_username_or_fetch(uuid: &str, data: &Data) -> Option<String> {
    if let Some(username) = resolve_username(uuid, data).await {
        return Some(username);
    }
    let username = data.api.resolve(uuid).await.ok()?.username;
    let _ = CacheRepository::new(data.db.pool())
        .cache_username(uuid, &username)
        .await;
    Some(username)
}

pub fn text(s: impl Into<String>) -> CreateContainerComponent<'static> {
    CreateContainerComponent::TextDisplay(CreateTextDisplay::new(s.into()))
}

pub fn separator() -> CreateContainerComponent<'static> {
    CreateContainerComponent::Separator(CreateSeparator::new(true))
}
