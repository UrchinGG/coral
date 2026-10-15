use std::sync::Arc;

use clients::{Extension, ExtensionContext};
use coral_redis::RedisPool;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionSummary {
    pub id: &'static str,
    pub title: &'static str,
}

pub fn load(redis: &RedisPool) -> Arc<[Arc<dyn Extension>]> {
    let context = ExtensionContext {
        redis: redis.connection(),
    };
    clients::extensions(context).into()
}

pub fn summaries(extensions: &[Arc<dyn Extension>]) -> Vec<ExtensionSummary> {
    extensions
        .iter()
        .map(|extension| ExtensionSummary {
            id: extension.id(),
            title: extension.title(),
        })
        .collect()
}
