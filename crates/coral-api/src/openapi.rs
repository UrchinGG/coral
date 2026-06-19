use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::{
    error::ErrorResponse,
    responses::*,
    routes::{
        batch::{BatchRequest, BatchResponse},
        cubelify::CubelifyQuery,
        guild::{GuildSessionResponse, GuildSnapshotDataResponse, GuildSnapshotListResponse},
        resolve::ResolveResponse,
        session::*,
        tags::{AddTagBody, LockRequest, RemoveTagBody, UpdateTagBody, UuidQuery},
        verify::{RedeemCodeResponse, StoreCodeRequest},
        winstreaks::*,
    },
};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::cubelify::get_cubelify,

        crate::routes::player::player_tags,
        crate::routes::player::player_face,
        crate::routes::player::player_body,
        crate::routes::batch::batch_lookup,
        crate::routes::session::session_daily,
        crate::routes::session::session_weekly,
        crate::routes::session::session_monthly,
        crate::routes::session::session_yearly,
        crate::routes::session::session_custom,
        crate::routes::session::list_markers,
        crate::routes::session::create_marker,
        crate::routes::session::rename_marker,
        crate::routes::session::delete_marker,
        crate::routes::session::list_snapshots,
        crate::routes::guild::guild_daily,
        crate::routes::guild::guild_weekly,
        crate::routes::guild::guild_monthly,
        crate::routes::guild::guild_yearly,
        crate::routes::guild::guild_custom,
        crate::routes::guild::guild_snapshots,
        crate::routes::winstreaks::player_winstreaks,

        crate::routes::tags::add_tag,
        crate::routes::tags::remove_tag,
        crate::routes::tags::update_tag,
        crate::routes::tags::lock_player,
        crate::routes::tags::unlock_player,

        crate::routes::player::player_stats,
        crate::routes::player::player_skin,
        crate::routes::hypixel::player,
        crate::routes::hypixel::guild,
        crate::routes::resolve::resolve_player,
        crate::routes::verify::store_code,
        crate::routes::verify::redeem_code,
        crate::health_check,
    ),
    components(
        schemas(
            ErrorResponse, SuccessResponse,
            CubelifyResponse, CubelifyQuery,
            PlayerTagsResponse, TagResponse,
            BatchRequest, BatchResponse,
            SessionDeltaResponse,
            MarkerResponse, MarkerListResponse,
            CreateMarkerRequest, RenameMarkerRequest,
            SnapshotListResponse, SnapshotDataResponse,
            GuildSessionResponse, GuildSnapshotListResponse, GuildSnapshotDataResponse,
            WinstreakResponse, StreakEntry,
            AddTagBody, RemoveTagBody, UpdateTagBody, LockRequest, UuidQuery,
            PlayerStatsResponse,
            ResolveResponse,
            StoreCodeRequest, RedeemCodeResponse,
        )
    ),
    tags(
        (name = "Cubelify", description = "Blacklist data formatted for the Cubelify overlay."),
        (name = "Player", description = "Hypixel player data, including tags, sessions, markers, winstreaks, and batch lookups."),
        (name = "Guild", description = "Historical guild snapshots and session deltas for guilds you are a member of."),
        (name = "Blacklist", description = "Blacklist write operations: adding, removing, and overwriting tags, and locking players."),
        (name = "Internal", description = "Privileged endpoints that require a developer permission or an Admin key."),
        (name = "Hypixel", description = "Unmodified passthrough to the Hypixel API."),
    ),
    modifiers(&DocsAddon),
    info(
        title = "Coral API",
        version = "0.1.0",
    ),
    servers(
        (url = "https://api.urchin.gg", description = "Production"),
    ),
)]
pub struct ApiDoc;

pub const API_OVERVIEW: &str = r#"Coral is an API for Hypixel player data and the Urchin cheater blacklist.

## Base URL

```
https://api.urchin.gg
```

Every endpoint here lives under `/v3`.

## Authentication

Every request requires an API key. Send it in the `X-API-Key` header, or as a `key` query parameter where setting a header is impractical:

```
X-API-Key: <your-key>
```
```
?key=<your-key>
```

A personal key is issued through the Discord bot and carries the access rank of your Urchin account. A developer key carries an explicit set of permissions: Player Data, Hypixel, and All Sessions. Each endpoint states the rank or permission it requires. An endpoint without such a note accepts any valid key. A locked key is rejected with a 403, and a missing or unknown key returns a 401.

## Rate limits

Each key is rate limited within a rolling five-minute window. A personal key allows 600 requests per window, while a developer key uses the limit assigned when it was issued. Once that limit is reached, further requests receive a 429 response until the window resets. Blacklist writes count against a separate per-account allowance that is determined by your rank.

## Players and timestamps

Endpoints that accept a player take a UUID, with or without dashes, or a username that is resolved through Mojang. Timestamps are returned as Unix milliseconds, usually accompanied by a human-readable form.

## Errors

Errors are returned as JSON with a single `error` field:

```json
{ "error": "player not found" }
```

| Status | Meaning |
| --- | --- |
| 400 | The request is malformed, such as a bad parameter, body, or identifier. |
| 401 | The API key is missing or unrecognized. |
| 403 | The key is locked, or your rank or permission is insufficient. |
| 404 | No resource matches the request. |
| 409 | The request conflicts with the current state, such as a tag the player already has. |
| 429 | You have exceeded your rate limit. |
| 502 | Hypixel or Mojang returned an upstream error. |
| 503 | A service that Coral depends on is unavailable. |

## Machine-readable docs

The complete OpenAPI specification is available at `https://api.urchin.gg/openapi.json`. For a concise plain-text summary intended for language models, see `https://api.urchin.gg/llms.txt`.
"#;

struct DocsAddon;

impl Modify for DocsAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.info.description = Some(API_OVERVIEW.to_string());

        let components = openapi
            .components
            .get_or_insert_with(utoipa::openapi::Components::new);
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-API-Key"))),
        );
    }
}

pub fn llms_txt(openapi: &utoipa::openapi::OpenApi) -> String {
    use std::fmt::Write;

    let mut out = format!(
        "# {}\n\n> Coral is an API for Hypixel player data and the Urchin cheater blacklist.\n\n{}\n## Endpoints\n",
        openapi.info.title, API_OVERVIEW
    );

    for (path, item) in &openapi.paths.paths {
        for (method, op) in operations(item) {
            let summary = op
                .summary
                .as_deref()
                .or(op.description.as_deref())
                .unwrap_or("");
            let _ = writeln!(out, "- `{method} {path}`: {summary}");
        }
    }

    out
}

fn operations(
    item: &utoipa::openapi::path::PathItem,
) -> Vec<(&'static str, &utoipa::openapi::path::Operation)> {
    [
        ("GET", &item.get),
        ("POST", &item.post),
        ("PUT", &item.put),
        ("PATCH", &item.patch),
        ("DELETE", &item.delete),
    ]
    .into_iter()
    .filter_map(|(method, op)| op.as_ref().map(|op| (method, op)))
    .collect()
}
