use blacklist::{permissions, priority_lane, priority_lane_excluding};
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::blacklist::{AddOutcome, BlacklistRepository, OverwriteOutcome, PlayerEvent};

#[derive(Debug)]
pub enum TagOpError {
    PlayerLocked,
    InsufficientPermissions,
    InvalidTagType,
    TagAlreadyExists,
    PriorityConflict(PlayerEvent),
    TagNotFound,
    EditWindowExpired,
    ModeratorRequired,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for TagOpError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

pub struct TagOp<'a> {
    repo: BlacklistRepository<'a>,
}

impl<'a> TagOp<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self {
            repo: BlacklistRepository::new(pool),
        }
    }

    pub fn repo(&self) -> &BlacklistRepository<'a> {
        &self.repo
    }

    pub async fn add(
        &self,
        uuid: &str,
        tag_type: &str,
        reason: &str,
        actor_id: i64,
        actor_level: i16,
        hide_username: bool,
        reviewed_by: Option<&[i64]>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<PlayerEvent, TagOpError> {
        if blacklist::lookup(tag_type).is_none() {
            return Err(TagOpError::InvalidTagType);
        }
        if !permissions::can_add(tag_type, actor_level) {
            return Err(TagOpError::InsufficientPermissions);
        }
        self.check_lock(uuid).await?;

        let hide = permissions::can_set_hide(actor_level) && hide_username;

        match self
            .repo
            .add_event(
                uuid,
                tag_type,
                reason,
                hide,
                expires_at,
                reviewed_by,
                Some(actor_id),
                &priority_lane(tag_type),
            )
            .await?
        {
            AddOutcome::Inserted(id) => self
                .repo
                .get_event_by_id(id)
                .await?
                .ok_or(TagOpError::Database(sqlx::Error::RowNotFound)),
            AddOutcome::Conflict(c) if c.tag_type.as_deref() == Some(tag_type) => {
                Err(TagOpError::TagAlreadyExists)
            }
            AddOutcome::Conflict(c) => Err(TagOpError::PriorityConflict(c)),
        }
    }

    pub async fn remove(
        &self,
        uuid: &str,
        tag_type: &str,
        actor_id: i64,
        actor_level: i16,
    ) -> Result<PlayerEvent, TagOpError> {
        let tag = self.require_active_tag(uuid, tag_type).await?;
        self.check_lock(uuid).await?;
        self.authorize_remove(&tag, actor_id, actor_level)?;
        if !self
            .repo
            .remove_event(uuid, tag_type, Some(actor_id))
            .await?
        {
            return Err(TagOpError::TagNotFound);
        }
        Ok(tag)
    }

    pub async fn remove_by_id(
        &self,
        event_id: i64,
        actor_id: i64,
        actor_level: i16,
    ) -> Result<(String, PlayerEvent), TagOpError> {
        let event = self
            .repo
            .get_event_by_id(event_id)
            .await?
            .ok_or(TagOpError::TagNotFound)?;
        if event.kind != "tag_set" {
            return Err(TagOpError::TagNotFound);
        }
        let tag_type = event.tag_type.clone().ok_or(TagOpError::TagNotFound)?;
        let active = self
            .repo
            .get_active_tag(&event.uuid, &tag_type)
            .await?
            .ok_or(TagOpError::TagNotFound)?;
        if active.id != event.id {
            return Err(TagOpError::TagNotFound);
        }

        self.check_lock(&event.uuid).await?;
        self.authorize_remove(&event, actor_id, actor_level)?;

        let uuid = event.uuid.clone();
        if !self
            .repo
            .remove_event(&uuid, &tag_type, Some(actor_id))
            .await?
        {
            return Err(TagOpError::TagNotFound);
        }
        Ok((uuid, event))
    }

    pub async fn overwrite(
        &self,
        uuid: &str,
        old_tag_type: &str,
        new_tag_type: &str,
        new_reason: &str,
        actor_id: i64,
        actor_level: i16,
        hide_username: bool,
    ) -> Result<(PlayerEvent, PlayerEvent), TagOpError> {
        if blacklist::lookup(new_tag_type).is_none() {
            return Err(TagOpError::InvalidTagType);
        }
        if !permissions::can_add(new_tag_type, actor_level) {
            return Err(TagOpError::InsufficientPermissions);
        }
        self.check_lock(uuid).await?;

        let old_preview = self.require_active_tag(uuid, old_tag_type).await?;
        self.authorize_remove(&old_preview, actor_id, actor_level)?;

        let hide = permissions::can_set_hide(actor_level) && hide_username;
        let blocking = if new_tag_type == old_tag_type {
            Vec::new()
        } else {
            priority_lane_excluding(new_tag_type, old_tag_type)
        };

        match self
            .repo
            .overwrite_event(
                uuid,
                old_tag_type,
                new_tag_type,
                new_reason,
                hide,
                None,
                None,
                Some(actor_id),
                &blocking,
                Some(old_preview.id),
            )
            .await?
        {
            OverwriteOutcome::Inserted { old, new } => Ok((old, new)),
            OverwriteOutcome::OldNotActive => Err(TagOpError::TagNotFound),
            OverwriteOutcome::Conflict(c) => Err(TagOpError::PriorityConflict(c)),
        }
    }

    pub async fn lock_player(
        &self,
        uuid: &str,
        reason: &str,
        actor_id: i64,
        actor_level: i16,
    ) -> Result<(), TagOpError> {
        if actor_level < 3 {
            return Err(TagOpError::ModeratorRequired);
        }
        self.repo.lock_event(uuid, Some(reason), actor_id).await?;
        Ok(())
    }

    pub async fn unlock_player(
        &self,
        uuid: &str,
        actor_id: i64,
        actor_level: i16,
    ) -> Result<bool, TagOpError> {
        if actor_level < 3 {
            return Err(TagOpError::ModeratorRequired);
        }
        Ok(self.repo.unlock_event(uuid, actor_id).await?)
    }

    async fn check_lock(&self, uuid: &str) -> Result<(), TagOpError> {
        if self.repo.get_lock_state(uuid).await?.locked {
            return Err(TagOpError::PlayerLocked);
        }
        Ok(())
    }

    async fn require_active_tag(
        &self,
        uuid: &str,
        tag_type: &str,
    ) -> Result<PlayerEvent, TagOpError> {
        self.repo
            .get_active_tag(uuid, tag_type)
            .await?
            .ok_or(TagOpError::TagNotFound)
    }

    fn authorize_remove(
        &self,
        tag: &PlayerEvent,
        actor_id: i64,
        actor_level: i16,
    ) -> Result<(), TagOpError> {
        let is_own = tag.author == Some(actor_id);
        let age = Utc::now().signed_duration_since(tag.ts).num_minutes();
        let tag_type = tag.tag_type.as_deref().unwrap_or("");
        if !permissions::can_remove(tag_type, actor_level, is_own, age) {
            return Err(TagOpError::InsufficientPermissions);
        }
        Ok(())
    }
}
