use database::{Database, MinecraftBlock, MinecraftBlockRepository};
use serde_json::json;
use uuid::Uuid;

use super::tags::{LockView, NewLock};
use crate::audit;
use crate::error::{AdminError, Context};
use crate::refs::EntityRef;

impl From<MinecraftBlock> for LockView {
    fn from(block: MinecraftBlock) -> Self {
        Self {
            by: Some(EntityRef::Account {
                discord_id: block.blocked_by,
                member_id: None,
            }),
            at: block.blocked_at,
            reason: block.reason,
        }
    }
}

pub async fn block(
    db: &Database,
    actor: i64,
    player: Uuid,
    block: NewLock,
) -> Result<(), AdminError> {
    let reason = block.reason();
    let blocked = MinecraftBlockRepository::new(db.pool())
        .block(player, reason, actor)
        .await
        .context("block the Minecraft account")?;
    if !blocked {
        return Err(AdminError::Rejected(
            "the Minecraft account is already blocked".into(),
        ));
    }
    let details = json!({ "reason": reason });
    audit::record(
        db.pool(),
        actor,
        "block_minecraft_account",
        &player.simple().to_string(),
        &details,
    )
    .await;
    Ok(())
}

pub async fn unblock(db: &Database, actor: i64, player: Uuid) -> Result<(), AdminError> {
    let unblocked = MinecraftBlockRepository::new(db.pool())
        .unblock(player)
        .await
        .context("unblock the Minecraft account")?;
    if !unblocked {
        return Err(AdminError::Rejected(
            "the Minecraft account is not blocked".into(),
        ));
    }
    audit::record(
        db.pool(),
        actor,
        "unblock_minecraft_account",
        &player.simple().to_string(),
        &json!({}),
    )
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blocks_toggle_once_and_are_audited() {
        let Some(db) = crate::players::fixtures::database().await else {
            return;
        };
        let player = Uuid::new_v4();
        let staff = 300_000_000_000_000_007;
        let spam = || serde_json::from_value(json!({ "reason": " spam " })).unwrap();

        block(&db, staff, player, spam()).await.unwrap();
        assert_eq!(
            block(&db, staff, player, spam()).await.unwrap_err().code(),
            "rejected"
        );
        let view = LockView::from(
            MinecraftBlockRepository::new(db.pool())
                .get(player)
                .await
                .unwrap()
                .unwrap(),
        );
        assert_eq!(view.reason.as_deref(), Some("spam"));

        unblock(&db, staff, player).await.unwrap();
        assert_eq!(
            unblock(&db, staff, player).await.unwrap_err().code(),
            "rejected"
        );
        let audited: Vec<String> =
            sqlx::query_scalar("SELECT action FROM admin_actions WHERE target = $1 ORDER BY id")
                .bind(player.simple().to_string())
                .fetch_all(db.pool())
                .await
                .unwrap();
        assert_eq!(
            audited,
            ["block_minecraft_account", "unblock_minecraft_account"]
        );
    }
}
