use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::error::ErrorResponse;
use crate::responses::{PlayerStatsResponse, PlayerTagsResponse, TagResponse, CubelifyResponse};
use crate::routes::batch::{BatchRequest, BatchResponse};
use crate::routes::tags::{AddTagRequest, LockRequest, OverwriteTagRequest};
use crate::routes::tags::{SuccessResponse, TagIdResponse};
use crate::routes::guild::{GuildQuery, GuildResponse};
use crate::routes::resolve::ResolveResponse;
use crate::routes::verify::{StoreCodeRequest, RedeemCodeResponse};
use crate::routes::cubelify::CubelifyQuery;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::player::player_tags,
        crate::routes::player::player_stats,
        crate::routes::player::player_skin,
        crate::routes::batch::batch_lookup,
        crate::routes::tags::add_tag,
        crate::routes::tags::remove_tag,
        crate::routes::tags::overwrite_tag,
        crate::routes::tags::lock_player,
        crate::routes::tags::unlock_player,
        crate::routes::guild::get_guild,
        crate::routes::resolve::resolve_player,
        crate::routes::verify::store_code,
        crate::routes::verify::redeem_code,
        crate::routes::cubelify::get_cubelify,
        crate::health_check,
    ),
    components(
        schemas(
            PlayerStatsResponse,
            PlayerTagsResponse,
            TagResponse,
            CubelifyResponse,
            ErrorResponse,
            BatchRequest,
            BatchResponse,
            AddTagRequest,
            LockRequest,
            OverwriteTagRequest,
            SuccessResponse,
            TagIdResponse,
            GuildQuery,
            GuildResponse,
            ResolveResponse,
            StoreCodeRequest,
            RedeemCodeResponse,
            CubelifyQuery,
        )
    ),
    modifiers(&SecurityAddon),
    info(
        title = "Coral API",
        description = "Unified Hypixel player data and Urchin blacklist API",
        version = "0.1.0",
    ),
    servers(
        (url = "/", description = "Current server"),
    ),
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.components = Some(utoipa::openapi::Components::new());
        if let Some(components) = &mut openapi.components {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
            );
        }
    }
}
