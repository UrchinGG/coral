use std::sync::Arc;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use serde::Serialize;
use serde_json::Value;

use crate::view::ExtensionView;

#[derive(Clone)]
pub struct ExtensionContext {
    pub redis: ConnectionManager,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("{0}")]
    Rejected(String),
    #[error("unknown action {0}")]
    UnknownAction(String),
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FlagLevel {
    Info,
    Warning,
    Danger,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionFlag {
    pub key: String,
    pub level: FlagLevel,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub message: String,
    pub view: ExtensionView,
    pub audit: Value,
}

#[async_trait]
pub trait Extension: Send + Sync {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    async fn view(&self) -> Result<ExtensionView, ExtensionError>;
    async fn act(&self, action: &str, input: Value) -> Result<ActionOutcome, ExtensionError>;
    async fn flags(&self) -> Result<Vec<ExtensionFlag>, ExtensionError>;
}

pub fn extensions(_ctx: ExtensionContext) -> Vec<Arc<dyn Extension>> {
    Vec::new()
}
