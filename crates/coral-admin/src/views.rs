use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use moka::Expiry;
use moka::future::Cache;
use serde::Serialize;

use crate::error::AdminError;

pub const FLAGS_TTL: Duration = Duration::from_secs(10);
pub const RECENT_SUMMARY_TTL: Duration = Duration::from_secs(10);
pub const LONG_SUMMARY_TTL: Duration = Duration::from_secs(60);
pub const ROUTES_TTL: Duration = Duration::from_secs(10 * 60);
pub const GUILD_POPULATION_TTL: Duration = Duration::from_secs(10 * 60);
pub const DISCORD_DIRECTORY_TTL: Duration = Duration::from_secs(10 * 60);
pub const RECENT_SUMMARY_SPAN: Duration = Duration::from_secs(24 * 3600);

const MAX_VIEWS: u64 = 1_000;
const JSON: HeaderValue = HeaderValue::from_static("application/json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Flags,
    RecentSummary,
    LongSummary,
    Routes,
    GuildPopulation,
    DiscordDirectory,
}

#[derive(Clone)]
pub struct ViewCache {
    views: Cache<ViewKey, CachedView>,
}

#[derive(Clone)]
pub struct CachedView {
    body: Bytes,
}

type ViewKey = (View, String);

struct ViewExpiry;

impl View {
    pub fn traffic_summary(span: Duration) -> Self {
        match span <= RECENT_SUMMARY_SPAN {
            true => Self::RecentSummary,
            false => Self::LongSummary,
        }
    }

    pub fn ttl(self) -> Duration {
        match self {
            Self::Flags => FLAGS_TTL,
            Self::RecentSummary => RECENT_SUMMARY_TTL,
            Self::LongSummary => LONG_SUMMARY_TTL,
            Self::Routes => ROUTES_TTL,
            Self::GuildPopulation => GUILD_POPULATION_TTL,
            Self::DiscordDirectory => DISCORD_DIRECTORY_TTL,
        }
    }
}

impl ViewCache {
    pub fn new() -> Self {
        Self {
            views: Cache::builder()
                .max_capacity(MAX_VIEWS)
                .expire_after(ViewExpiry)
                .build(),
        }
    }

    pub async fn get_or_load<T, F, Fut>(
        &self,
        view: View,
        key: impl Into<String>,
        load: F,
    ) -> Result<CachedView, AdminError>
    where
        T: Serialize,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, AdminError>>,
    {
        self.views
            .try_get_with((view, key.into()), async move {
                let value = load().await?;
                CachedView::serialize(&value)
            })
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    pub async fn forget(&self, view: View, key: impl Into<String>) {
        self.views.invalidate(&(view, key.into())).await;
    }
}

impl CachedView {
    fn serialize<T: Serialize>(value: &T) -> Result<Self, AdminError> {
        let body = serde_json::to_vec(value)
            .map_err(|error| AdminError::Internal(format!("serialize a cached view: {error}")))?;
        Ok(Self {
            body: Bytes::from(body),
        })
    }
}

impl IntoResponse for CachedView {
    fn into_response(self) -> Response {
        ([(header::CONTENT_TYPE, JSON)], self.body).into_response()
    }
}

impl Expiry<ViewKey, CachedView> for ViewExpiry {
    fn expire_after_create(
        &self,
        key: &ViewKey,
        _value: &CachedView,
        _created_at: Instant,
    ) -> Option<Duration> {
        Some(key.0.ttl())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::body::to_bytes;

    use super::*;

    const BODY_LIMIT: usize = 1024;

    async fn body(view: CachedView) -> String {
        let bytes = to_bytes(view.into_response().into_body(), BODY_LIMIT)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn loads_once_and_serves_the_cached_json() {
        let cache = ViewCache::new();
        let loads = AtomicUsize::new(0);
        for _ in 0..3 {
            let view = cache
                .get_or_load(View::Routes, "all", || async {
                    loads.fetch_add(1, Ordering::SeqCst);
                    Ok(vec!["/v3/players"])
                })
                .await
                .unwrap();
            assert_eq!(body(view).await, r#"["/v3/players"]"#);
        }
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_loads_are_returned_and_not_cached() {
        let cache = ViewCache::new();
        let failed = cache
            .get_or_load(View::Flags, "", || async {
                Err::<u8, _>(AdminError::Timeout("flags query".into()))
            })
            .await;
        assert_eq!(
            failed.err(),
            Some(AdminError::Timeout("flags query".into()))
        );
        let loaded = cache
            .get_or_load(View::Flags, "", || async { Ok(1) })
            .await
            .unwrap();
        assert_eq!(body(loaded).await, "1");
    }

    #[tokio::test]
    async fn keys_separate_views_and_variants() {
        let cache = ViewCache::new();
        let a = cache
            .get_or_load(View::RecentSummary, "1h", || async { Ok("a") })
            .await
            .unwrap();
        let b = cache
            .get_or_load(View::RecentSummary, "6h", || async { Ok("b") })
            .await
            .unwrap();
        assert_eq!(body(a).await, r#""a""#);
        assert_eq!(body(b).await, r#""b""#);
    }

    #[test]
    fn summaries_beyond_a_day_cache_longer() {
        assert_eq!(
            View::traffic_summary(Duration::from_secs(3600)),
            View::RecentSummary
        );
        assert_eq!(
            View::traffic_summary(Duration::from_secs(7 * 86_400)),
            View::LongSummary
        );
        assert_eq!(View::LongSummary.ttl(), LONG_SUMMARY_TTL);
    }

    #[test]
    fn each_view_expires_after_its_own_ttl() {
        for view in [
            View::Flags,
            View::RecentSummary,
            View::LongSummary,
            View::Routes,
            View::GuildPopulation,
            View::DiscordDirectory,
        ] {
            let expiry = ViewExpiry.expire_after_create(
                &(view, String::new()),
                &CachedView::serialize(&1).unwrap(),
                Instant::now(),
            );
            assert_eq!(expiry, Some(view.ttl()), "{view:?}");
        }
    }
}
