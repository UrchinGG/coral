use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use database::AccessRank;
use serde::Serialize;

use crate::auth::AdminActor;
use crate::security::json_error;

/// What a signed-in staff member may see or do; each is held from one Urchin access rank up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Staff,
    PlayersRead,
    TagsWrite,
    MembersRead,
    TaggingWrite,
    StrikesWrite,
    AccessLevelWrite,
    PlayerLocksWrite,
    SignInBlocksWrite,
    KeyLimitsWrite,
    ReviewsManage,
    KeysManage,
    Traffic,
    Starfish,
    Owner,
}

const ANY: Option<Method> = None;
const GET: Option<Method> = Some(Method::GET);
const POST: Option<Method> = Some(Method::POST);
const DELETE: Option<Method> = Some(Method::DELETE);

/// Routes below the owner, matched by method and path with `*` for one segment.
/// A route missing here is owner-only, so a new endpoint stays closed until it is placed.
const ROUTES: &[(Option<Method>, &str, Permission)] = &[
    (GET, "/session", Permission::Staff),
    (POST, "/search", Permission::Staff),
    (POST, "/search/remote", Permission::Staff),
    (POST, "/labels", Permission::Staff),
    (GET, "/overview", Permission::Staff),
    (GET, "/moderation/recent", Permission::Staff),
    (GET, "/players/recently-tagged", Permission::PlayersRead),
    (GET, "/players/top", Permission::Traffic),
    (GET, "/players/*", Permission::PlayersRead),
    (GET, "/players/*/tags", Permission::PlayersRead),
    (GET, "/players/*/snapshots", Permission::PlayersRead),
    (GET, "/players/*/snapshots/at", Permission::PlayersRead),
    (POST, "/players/*/tags", Permission::TagsWrite),
    (DELETE, "/players/*/tags/*", Permission::TagsWrite),
    (POST, "/players/*/lock", Permission::PlayerLocksWrite),
    (POST, "/players/*/unlock", Permission::PlayerLocksWrite),
    (POST, "/players/*/block", Permission::SignInBlocksWrite),
    (POST, "/players/*/unblock", Permission::SignInBlocksWrite),
    (GET, "/players/*/requests", Permission::Traffic),
    (GET, "/players/*/requesters", Permission::Traffic),
    (GET, "/players/*/lookups", Permission::Traffic),
    (GET, "/guilds", Permission::PlayersRead),
    (GET, "/guilds/*", Permission::PlayersRead),
    (GET, "/guilds/*/at", Permission::PlayersRead),
    (GET, "/accounts", Permission::MembersRead),
    (GET, "/accounts/by-member/*", Permission::MembersRead),
    (GET, "/accounts/*", Permission::MembersRead),
    (GET, "/accounts/*/timeline", Permission::MembersRead),
    (POST, "/accounts/*/tagging", Permission::TaggingWrite),
    (POST, "/accounts/*/strikes", Permission::StrikesWrite),
    (DELETE, "/accounts/*/strikes/*", Permission::StrikesWrite),
    (
        POST,
        "/accounts/*/access-level",
        Permission::AccessLevelWrite,
    ),
    (GET, "/accounts/*/limits", Permission::KeyLimitsWrite),
    (GET, "/accounts/*/waste", Permission::KeyLimitsWrite),
    (POST, "/accounts/*/limits/reset", Permission::KeyLimitsWrite),
    (
        DELETE,
        "/accounts/*/waste-block",
        Permission::KeyLimitsWrite,
    ),
    (ANY, "/moderation/guide", Permission::ReviewsManage),
    (ANY, "/moderation/guide/*", Permission::ReviewsManage),
    (ANY, "/moderation/rules", Permission::ReviewsManage),
    (GET, "/moderation/directory", Permission::ReviewsManage),
    (GET, "/moderation/population", Permission::ReviewsManage),
];

impl Permission {
    pub const ALL: [Self; 15] = [
        Self::Staff,
        Self::PlayersRead,
        Self::TagsWrite,
        Self::MembersRead,
        Self::TaggingWrite,
        Self::StrikesWrite,
        Self::AccessLevelWrite,
        Self::PlayerLocksWrite,
        Self::SignInBlocksWrite,
        Self::KeyLimitsWrite,
        Self::ReviewsManage,
        Self::KeysManage,
        Self::Traffic,
        Self::Starfish,
        Self::Owner,
    ];

    pub const fn minimum(self) -> AccessRank {
        match self {
            Self::Staff
            | Self::PlayersRead
            | Self::TagsWrite
            | Self::MembersRead
            | Self::TaggingWrite => AccessRank::Helper,
            Self::StrikesWrite | Self::PlayerLocksWrite | Self::SignInBlocksWrite => {
                AccessRank::Moderator
            }
            Self::AccessLevelWrite | Self::KeyLimitsWrite | Self::ReviewsManage => {
                AccessRank::Admin
            }
            Self::KeysManage | Self::Traffic | Self::Starfish | Self::Owner => AccessRank::Owner,
        }
    }

    pub fn granted(rank: AccessRank) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|permission| rank >= permission.minimum())
            .collect()
    }

    pub fn for_route(method: &Method, path: &str) -> Self {
        ROUTES
            .iter()
            .find(|(allowed, pattern, _)| {
                allowed.as_ref().is_none_or(|allowed| allowed == method) && matches(pattern, path)
            })
            .map_or(Self::Owner, |(_, _, permission)| *permission)
    }
}

impl AdminActor {
    pub fn can(&self, permission: Permission) -> bool {
        self.rank >= permission.minimum()
    }

    /// Bot rule: staff act only on members ranked below them; owners act on anyone.
    pub fn outranks(&self, target: AccessRank) -> bool {
        self.rank == AccessRank::Owner || self.rank > target
    }

    /// Bot rule: a new rank stays below the granter's own, and only admins move anyone into
    /// or out of a staff rank.
    pub fn may_set_rank(&self, from: AccessRank, to: AccessRank) -> bool {
        let staff_change = from >= AccessRank::Helper || to >= AccessRank::Helper;
        to < self.rank && (!staff_change || self.rank >= AccessRank::Admin)
    }
}

pub async fn authorize(request: Request, next: Next) -> Response {
    let Some(actor) = request.extensions().get::<AdminActor>().copied() else {
        return json_error(StatusCode::UNAUTHORIZED, "unauthorized", "Sign in as staff");
    };
    let needed = Permission::for_route(request.method(), request.uri().path());
    if !actor.can(needed) {
        return json_error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Your staff rank does not include this",
        );
    }
    next.run(request).await
}

fn matches(pattern: &str, path: &str) -> bool {
    let mut pattern = pattern.trim_matches('/').split('/');
    let mut path = path
        .trim_end_matches('/')
        .trim_start_matches('/')
        .split('/');
    loop {
        match (pattern.next(), path.next()) {
            (None, None) => return true,
            (Some("*"), Some(segment)) if !segment.is_empty() => {}
            (Some(expected), Some(segment)) if expected == segment => {}
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(rank: AccessRank) -> AdminActor {
        AdminActor {
            discord_id: 1,
            rank,
        }
    }

    fn needs(method: Method, path: &str) -> AccessRank {
        Permission::for_route(&method, path).minimum()
    }

    #[test]
    fn routes_map_to_the_tier_the_bot_uses() {
        assert_eq!(needs(Method::GET, "/players/abc"), AccessRank::Helper);
        assert_eq!(needs(Method::POST, "/players/abc/tags"), AccessRank::Helper);
        assert_eq!(
            needs(Method::POST, "/accounts/1/strikes"),
            AccessRank::Moderator
        );
        assert_eq!(
            needs(Method::DELETE, "/accounts/1/strikes/9"),
            AccessRank::Moderator
        );
        assert_eq!(needs(Method::PUT, "/moderation/rules"), AccessRank::Admin);
        assert_eq!(
            needs(Method::DELETE, "/accounts/1/waste-block"),
            AccessRank::Admin
        );
    }

    #[test]
    fn traffic_keys_and_unlisted_routes_stay_with_the_owner() {
        for (method, path) in [
            (Method::GET, "/players/top"),
            (Method::GET, "/players/abc/lookups"),
            (Method::GET, "/accounts/1/network"),
            (Method::GET, "/accounts/1/keys"),
            (Method::POST, "/accounts/1/dev-key"),
            (Method::POST, "/accounts/1/lock"),
            (Method::GET, "/traffic/summary"),
            (Method::GET, "/sync/servers"),
            (Method::GET, "/extensions"),
            (Method::GET, "/starfish/users"),
            (Method::POST, "/players/abc"),
            (Method::GET, "/players/abc/tags/extra"),
        ] {
            assert_eq!(needs(method, path), AccessRank::Owner, "{path}");
        }
    }

    #[test]
    fn wildcards_cover_exactly_one_segment() {
        assert!(matches("/players/*", "/players/abc"));
        assert!(matches("/players/*", "/players/abc/"));
        assert!(!matches("/players/*", "/players"));
        assert!(!matches("/players/*", "/players//"));
        assert!(!matches("/players/*", "/players/abc/tags"));
    }

    #[test]
    fn staff_act_only_below_their_rank_and_grant_like_the_bot() {
        let moderator = actor(AccessRank::Moderator);
        assert!(moderator.outranks(AccessRank::Helper));
        assert!(!moderator.outranks(AccessRank::Moderator));
        assert!(!moderator.may_set_rank(AccessRank::Default, AccessRank::Helper));
        assert!(!moderator.may_set_rank(AccessRank::Helper, AccessRank::Default));
        let admin = actor(AccessRank::Admin);
        assert!(admin.may_set_rank(AccessRank::Default, AccessRank::Moderator));
        assert!(admin.may_set_rank(AccessRank::Moderator, AccessRank::Default));
        assert!(!admin.may_set_rank(AccessRank::Default, AccessRank::Admin));
        let owner = actor(AccessRank::Owner);
        assert!(owner.outranks(AccessRank::Owner));
        assert!(owner.may_set_rank(AccessRank::Default, AccessRank::Admin));
        assert!(!owner.may_set_rank(AccessRank::Default, AccessRank::Owner));
    }

    #[test]
    fn each_tier_holds_everything_below_it() {
        let helper = Permission::granted(AccessRank::Helper);
        assert!(helper.contains(&Permission::TagsWrite));
        assert!(!helper.contains(&Permission::StrikesWrite));
        assert!(Permission::granted(AccessRank::Default).is_empty());
        assert_eq!(
            Permission::granted(AccessRank::Owner).len(),
            Permission::ALL.len()
        );
    }
}
