use axum::extract::State;
use axum::{Extension, Json};

use crate::auth::AdminActor;
use crate::boot::Boot;
use crate::state::AppState;

pub async fn boot(
    State(state): State<AppState>,
    Extension(actor): Extension<AdminActor>,
) -> Json<Boot> {
    Json(state.boot.boot(actor.discord_id))
}
