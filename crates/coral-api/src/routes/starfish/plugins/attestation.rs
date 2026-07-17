use std::collections::HashMap;

use axum::{Json, extract::State};

use database::PluginRegistryRepository;

use crate::{error::ApiError, state::AppState};

use super::dto::{AttestationInfoRequest, AttestationInfoResponse, PluginAttestationInfoDto};

const MAX_SLUGS_PER_REQUEST: usize = 100;

pub async fn attestation_info(
    State(state): State<AppState>,
    Json(req): Json<AttestationInfoRequest>,
) -> Result<Json<AttestationInfoResponse>, ApiError> {
    if req.slugs.len() > MAX_SLUGS_PER_REQUEST {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_SLUGS_PER_REQUEST} slugs per request"
        )));
    }

    let repo = PluginRegistryRepository::new(state.db.pool());
    let mut plugins = HashMap::with_capacity(req.slugs.len());

    for slug in &req.slugs {
        let Some(plugin) = repo.get_plugin_by_slug(slug).await? else {
            continue;
        };
        if !plugin.official {
            plugins.insert(
                slug.clone(),
                PluginAttestationInfoDto {
                    official: false,
                    content_sha256: None,
                },
            );
            continue;
        }
        let content_sha256 = repo
            .get_latest_release(plugin.id)
            .await?
            .and_then(|release| release.content_sha256)
            .map(hex::encode);

        plugins.insert(
            slug.clone(),
            PluginAttestationInfoDto {
                official: true,
                content_sha256,
            },
        );
    }

    Ok(Json(AttestationInfoResponse { plugins }))
}
