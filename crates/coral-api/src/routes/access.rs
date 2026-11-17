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
    auth::{AuthenticatedMember, DeveloperKeyAuth, MinecraftAuth, SessionAuth},
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
    let can_tag = credentials_can_tag(extensions, in_urchin_server);
    let addable_tags = tag_names(blacklist::user_addable(), |tag| {
        can_tag && !member.tagging_disabled && can_add(tag, level)
    });
    let removable_tags = tag_names(blacklist::all(), |_| can_tag && can_remove_any(level));
    Ok(MemberAccess {
        discord_id: member.discord_id.to_string(),
        discord_username,
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

fn credentials_can_tag(extensions: &Extensions, in_urchin_server: Option<bool>) -> bool {
    let key_allows = extensions
        .get::<DeveloperKeyAuth>()
        .is_none_or(|key| key.has(database::permissions::TAGS));
    let needs_server =
        extensions.get::<MinecraftAuth>().is_some() || extensions.get::<SessionAuth>().is_some();
    key_allows && !(needs_server && in_urchin_server == Some(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn with<T: Clone + Send + Sync + 'static>(auth: T) -> Extensions {
        let mut extensions = Extensions::new();
        extensions.insert(auth);
        extensions
    }

    #[test]
    fn developer_keys_tag_only_with_the_tags_permission() {
        let granted = with(DeveloperKeyAuth {
            permissions: database::permissions::TAGS,
        });
        let denied = with(DeveloperKeyAuth { permissions: 0 });
        assert!(credentials_can_tag(&granted, Some(false)));
        assert!(!credentials_can_tag(&denied, Some(true)));
    }

    #[test]
    fn minecraft_and_session_sign_ins_need_the_urchin_server() {
        for extensions in [
            with(MinecraftAuth { uuid: Uuid::nil() }),
            with(SessionAuth { discord_id: 1 }),
        ] {
            assert!(credentials_can_tag(&extensions, Some(true)));
            assert!(credentials_can_tag(&extensions, None));
            assert!(!credentials_can_tag(&extensions, Some(false)));
        }
    }

    #[test]
    fn personal_keys_tag_outside_the_urchin_server() {
        assert!(credentials_can_tag(&Extensions::new(), Some(false)));
    }
}
