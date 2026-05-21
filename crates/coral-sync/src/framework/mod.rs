use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serenity::all::*;
use serenity::async_trait;

use database::{Database, Member};

use crate::api::CoralApiClient;
use crate::commands;

pub use database::AccessRank;

pub trait AccessRankExt {
    fn of(data: &Data, user_id: u64, member: Option<&Member>) -> AccessRank;
}

impl AccessRankExt for AccessRank {
    fn of(_data: &Data, _user_id: u64, member: Option<&Member>) -> AccessRank {
        AccessRank::from_level(member.map(|m| m.access_level).unwrap_or(0))
    }
}

#[derive(Clone)]
pub struct Data {
    pub db: Arc<Database>,
    pub api: Arc<CoralApiClient>,
    pub owner_ids: Vec<u64>,
    pub home_guild_id: Option<GuildId>,
    pub redis_url: String,
    pub sync_cooldowns: Arc<Mutex<HashMap<UserId, Instant>>>,
    pub sync_cancel_tokens: Arc<Mutex<HashMap<GuildId, crate::sync::CancelToken>>>,
    pub active_interactions: Arc<std::sync::atomic::AtomicUsize>,
}

impl Data {
    pub fn is_owner(&self, user_id: u64) -> bool {
        self.owner_ids.contains(&user_id)
    }
}

pub struct Handler {
    data: Data,
}

impl Handler {
    pub fn new(data: Data) -> Self {
        Self { data }
    }

    fn commands() -> Vec<CreateCommand<'static>> {
        vec![
            commands::admin::setup::register()
                .integration_types(vec![InstallationContext::Guild])
                .contexts(vec![InteractionContext::Guild]),
        ]
    }

    async fn handle_command(
        &self,
        ctx: &Context,
        command: &CommandInteraction,
    ) -> anyhow::Result<()> {
        match command.data.name.as_str() {
            "setup" => commands::admin::setup::run(ctx, command, &self.data).await,
            _ => Ok(()),
        }
    }

    async fn handle_component(
        &self,
        ctx: &Context,
        component: &ComponentInteraction,
    ) -> anyhow::Result<()> {
        let id = component.data.custom_id.as_str();
        tracing::debug!("component interaction: {id}");

        match id {
            "setup_link" => {
                commands::admin::setup::handle_link_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_link_role_select:") => {
                commands::admin::setup::handle_link_role_select(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_unlinked_role_select:") => {
                commands::admin::setup::handle_unlinked_role_select(ctx, component, &self.data)
                    .await
            }
            _ if id.starts_with("setup_nickname_edit:") => {
                commands::admin::setup::handle_nickname_edit_button(ctx, component, &self.data)
                    .await
            }
            _ if id.starts_with("setup_nickname_clear:") => {
                commands::admin::setup::handle_nickname_clear_button(ctx, component, &self.data)
                    .await
            }
            _ if id.starts_with("setup_nickname:") => {
                commands::admin::setup::handle_nickname_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_link_channel_select:") => {
                commands::admin::setup::handle_link_channel_select(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_autorole:") => {
                commands::admin::setup::handle_autorole_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_role_config:") => {
                commands::admin::setup::handle_role_config_select(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_condition_edit:") => {
                commands::admin::setup::handle_condition_edit_button(ctx, component, &self.data)
                    .await
            }
            _ if id.starts_with("setup_rule_edit:") => {
                commands::admin::setup::handle_rule_edit_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_rule_remove:") => {
                commands::admin::setup::handle_rule_remove_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_role_strip:") => {
                commands::admin::setup::handle_role_strip_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_nickname_reset:") => {
                commands::admin::setup::handle_nickname_reset_button(ctx, component, &self.data)
                    .await
            }
            _ if id.starts_with("setup_autorole_back:") => {
                commands::admin::setup::handle_cancel_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_autorole_cancel:") => {
                commands::admin::setup::handle_autorole_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_sync_cancel:") => {
                commands::admin::setup::handle_sync_cancel_button(ctx, component, &self.data).await
            }
            _ if id.starts_with("setup_cancel:") => {
                commands::admin::setup::handle_cancel_button(ctx, component, &self.data).await
            }
            _ => {
                tracing::warn!("unhandled component interaction: {id}");
                Ok(())
            }
        }
    }

    async fn handle_modal(&self, ctx: &Context, modal: &ModalInteraction) -> anyhow::Result<()> {
        let id = modal.data.custom_id.as_str();

        match id {
            _ if id.starts_with("setup_nickname_modal:") => {
                commands::admin::setup::handle_nickname_modal(ctx, modal, &self.data).await
            }
            _ if id.starts_with("setup_add_rule_modal:") => {
                commands::admin::setup::handle_add_rule_modal(ctx, modal, &self.data).await
            }
            _ if id.starts_with("setup_rule_edit_modal:") => {
                commands::admin::setup::handle_rule_edit_modal(ctx, modal, &self.data).await
            }
            _ => Ok(()),
        }
    }

    async fn handle_interaction(&self, ctx: &Context, interaction: Interaction) {
        let result = match &interaction {
            Interaction::Command(command) => self.handle_command(ctx, command).await,
            Interaction::Component(component) => self.handle_component(ctx, component).await,
            Interaction::Modal(modal) => self.handle_modal(ctx, modal).await,
            _ => return,
        };

        if let Err(e) = result {
            tracing::error!("Interaction error: {e}");

            let container = CreateComponent::Container(
                CreateContainer::new(vec![crate::utils::text(
                    "## Something went wrong\nAn unexpected error occurred. Please try again later.",
                )])
                .accent_color(crate::interact::COLOR_ERROR),
            );
            let response = CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .components(vec![container]),
            );

            let error_response_result = match interaction {
                Interaction::Command(cmd) => cmd.create_response(&ctx.http, response).await,
                Interaction::Component(cmp) => cmp.create_response(&ctx.http, response).await,
                Interaction::Modal(modal) => modal.create_response(&ctx.http, response).await,
                _ => Ok(()),
            };
            if let Err(e) = error_response_result {
                tracing::error!("Failed to send error response: {e}");
            }
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn dispatch(&self, ctx: &Context, event: &FullEvent) {
        match event {
            FullEvent::Ready { data_about_bot, .. } => {
                tracing::info!("Bot connected as {}", data_about_bot.user.name);
                match Command::set_global_commands(&ctx.http, &Self::commands()).await {
                    Ok(cmds) => tracing::info!("Registered {} global commands", cmds.len()),
                    Err(e) => tracing::error!("Failed to register global commands: {}", e),
                }
                let ctx = ctx.clone();
                let data = self.data.clone();
                crate::events::spawn_sync_subscriber(ctx.clone(), data.clone());
                tokio::spawn(async move { crate::sync::startup_sync(ctx, data).await });
            }
            FullEvent::InteractionCreate { interaction, .. } => {
                self.data
                    .active_interactions
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.handle_interaction(ctx, interaction.clone()).await;
                self.data
                    .active_interactions
                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
            FullEvent::GuildMemberAddition { new_member, .. } => {
                if let Err(e) =
                    commands::user::link::handle_guild_join(ctx, new_member, &self.data).await
                {
                    tracing::error!("Guild join handler error: {}", e);
                }
            }
            FullEvent::GuildMemberUpdate { event, .. } => {
                if !event.user.bot() && event.user.id != ctx.cache.current_user().id {
                    let ctx = ctx.clone();
                    let data = self.data.clone();
                    let guild_id = event.guild_id;
                    let user_id = event.user.id;
                    tokio::spawn(async move {
                        match guild_id.member(&ctx.http, user_id).await {
                            Ok(member) => {
                                crate::sync::handle_member_update(&ctx, &data, &member).await
                            }
                            Err(e) => tracing::debug!("Failed to fetch member for update: {e}"),
                        }
                    });
                }
            }
            FullEvent::Message { new_message, .. } => {
                crate::sync::handle_message_activity(ctx, &self.data, new_message);
            }
            _ => {}
        }
    }
}
