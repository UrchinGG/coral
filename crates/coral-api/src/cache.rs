use database::CacheRepository;

use crate::state::AppState;

pub const SNAPSHOT_SOURCE: &str = "api";

pub async fn refresh_player_cache(state: &AppState, uuid: &str, username: Option<&str>) {
    let Some(hypixel) = state.hypixel.as_deref() else {
        return;
    };
    let Ok(Some(data)) = hypixel.get_player(uuid).await else {
        return;
    };
    let _ = CacheRepository::new(state.db.pool())
        .store_snapshot(uuid, &data, None, Some(SNAPSHOT_SOURCE), username)
        .await;
}
