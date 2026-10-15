use std::time::Duration;

use clients::{ClientError, PlayerIdentity};
use coral_redis::RateLimiter;

use super::Searcher;
use super::input::is_mojang_name;
use super::probe;
use super::rank::{self, SearchHit};
use crate::error::{AdminError, Context};
use crate::names::{MOJANG_ADMIN_BUCKET, MOJANG_ADMIN_BUDGET};

pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(3);

impl Searcher<'_> {
    pub async fn remote(&self, name: &str) -> Result<Vec<SearchHit>, AdminError> {
        if !is_mojang_name(name) {
            return Err(AdminError::BadRequest(
                "Mojang knows only names of 3 to 16 letters, digits and underscores".into(),
            ));
        }
        self.take_admin_budget().await?;
        let Some(identity) = self.mojang_identity(name).await? else {
            return Ok(Vec::new());
        };
        let uuid = reqlog::parse_uuid(&identity.uuid)
            .ok_or_else(|| AdminError::Upstream("Mojang answered with a malformed UUID".into()))?;
        let _permit = self.limits.probe().await;
        let owners = probe::player_owners(self.db, &[uuid])
            .await?
            .remove(&uuid)
            .unwrap_or_default();
        Ok(vec![rank::mojang_hit(uuid, &owners, &identity.username)])
    }

    async fn take_admin_budget(&self) -> Result<(), AdminError> {
        let granted = RateLimiter::new(self.redis.clone())
            .try_acquire(MOJANG_ADMIN_BUCKET, MOJANG_ADMIN_BUDGET)
            .await
            .context("take a lookup from the admin's Mojang budget")?;
        match granted {
            true => Ok(()),
            false => Err(AdminError::Rejected(
                "the admin's Mojang budget is spent for now; try again in a few minutes".into(),
            )),
        }
    }

    async fn mojang_identity(&self, name: &str) -> Result<Option<PlayerIdentity>, AdminError> {
        let lookup = tokio::time::timeout(REMOTE_TIMEOUT, self.names.mojang().resolve(name));
        match lookup.await {
            Err(_) => Err(AdminError::Timeout(format!(
                "Mojang did not answer within {} s",
                REMOTE_TIMEOUT.as_secs()
            ))),
            Ok(Ok(identity)) => Ok(Some(identity)),
            Ok(Err(ClientError::PlayerNotFound(_))) => Ok(None),
            Ok(Err(ClientError::RateLimited)) => Err(AdminError::Upstream(
                "Mojang rate limited the admin's lookup".into(),
            )),
            Ok(Err(error)) => Err(AdminError::Upstream(format!(
                "Mojang lookup failed: {error}"
            ))),
        }
    }
}
