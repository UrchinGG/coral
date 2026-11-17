use axum::extract::State;
use axum::http::Extensions;
use axum::routing::get;
use axum::{Json, Router};
use blacklist::TagDefinition;
use blacklist::permissions::{can_add, can_remove_any};
use database::{AccessRank, Member};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    auth::{AuthenticatedMember, DeveloperKeyAuth},
    error::ApiError,
    routes::tags::effective_level,
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub struct AccessResponse {
    pub member: Option<MemberAccess>,
}

#[derive(Serialize, ToSchema)]
pub struct MemberAccess {
    pub discord_id: String,
    pub discord_username: Option<String>,
    pub minecraft_linked: bool,
    pub in_urchin_server: Option<bool>,
    pub suspended: bool,
    pub tagging_disabled: bool,
    pub addable_tags: Vec<&'static str>,
    pub removable_tags: Vec<&'static str>,
    pub reveals_hidden_taggers: bool,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/access", get(access))
}

#[utoipa::path(
    get,
    path = "/v3/access",
    responses(
        (status = 200, description = "What the caller's credentials can do. `member` is null when no Urchin member is linked to them; `in_urchin_server` is null when Discord could not be asked", body = AccessResponse),
        (status = 403, description = "The key or account is locked", body = crate::error::ErrorResponse),
    ),
    tag = "Authentication", security(("api_key" = []))
)]
pub async fn access(
    State(state): State<AppState>,
    extensions: Extensions,
) -> Result<Json<AccessResponse>, ApiError> {
    let member = match extensions.get::<AuthenticatedMember>() {
        Some(AuthenticatedMember(member)) => {
            Some(member_access(&state, &extensions, member).await?)
        }
        None => None,
    };
    Ok(Json(AccessResponse { member }))
}

async fn member_access(
    state: &AppState,
    extensions: &Extensions,
    member: &Member,
) -> Result<MemberAccess, ApiError> {
    let discord_id = member.discord_id as u64;
    let (level, discord_username, in_urchin_server) = tokio::join!(
        effective_level(state, member),
        state.discord.resolve_username(discord_id),
        in_urchin_server(state, discord_id),
    );
    let level = level?;
    let minecraft_linked = member.uuid.is_some();
    let can_tag = credentials_can_tag(extensions, minecraft_linked, in_urchin_server);
    let addable_tags = tag_names(blacklist::user_addable(), |tag| {
        can_tag && !member.tagging_disabled && can_add(tag, level)
    });
    let removable_tags = tag_names(blacklist::all(), |_| can_tag && can_remove_any(level));
    Ok(MemberAccess {
        discord_id: member.discord_id.to_string(),
        discord_username,
        minecraft_linked,
        in_urchin_server,
        suspended: level < 0,
        tagging_disabled: member.tagging_disabled,
        addable_tags,
        removable_tags,
        reveals_hidden_taggers: AccessRank::from_level(member.access_level) >= AccessRank::Helper,
    })
}

fn tag_names(
    tags: &'static [&'static TagDefinition],
    allowed: impl Fn(&str) -> bool,
) -> Vec<&'static str> {
    tags.iter()
        .map(|tag| tag.name)
        .filter(|name| allowed(name))
        .collect()
}

async fn in_urchin_server(state: &AppState, discord_id: u64) -> Option<bool> {
    state
        .discord
        .is_recent_guild_member(state.home_guild_id?, discord_id)
        .await
}

fn credentials_can_tag(
    extensions: &Extensions,
    minecraft_linked: bool,
    in_urchin_server: Option<bool>,
) -> bool {
    match extensions.get::<DeveloperKeyAuth>() {
        Some(key) => key.has(database::permissions::TAGS),
        None => minecraft_linked && in_urchin_server != Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn developer_key(permissions: i64) -> Extensions {
        let mut extensions = Extensions::new();
        extensions.insert(DeveloperKeyAuth { permissions });
        extensions
    }

    #[test]
    fn developer_keys_tag_only_with_the_tags_permission() {
        let granted = developer_key(database::permissions::TAGS);
        assert!(credentials_can_tag(&granted, false, Some(false)));
        assert!(!credentials_can_tag(&developer_key(0), true, Some(true)));
    }

    #[test]
    fn member_credentials_need_the_urchin_server() {
        let personal = Extensions::new();
        assert!(credentials_can_tag(&personal, true, Some(true)));
        assert!(credentials_can_tag(&personal, true, None));
        assert!(!credentials_can_tag(&personal, true, Some(false)));
    }

    #[test]
    fn unlinked_members_never_tag() {
        assert!(!credentials_can_tag(&Extensions::new(), false, Some(true)));
    }
}
