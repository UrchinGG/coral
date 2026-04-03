use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use image::DynamicImage;
use moka::future::Cache;
use reqwest::Client;

use render::skin::{OutputType, Pose, Renderer, Skin};

use crate::MojangClient;

const USER_AGENT: &str = "Coral/1.0";
const CACHE_TTL_SECS: u64 = 10 * 60;
const CACHE_CAPACITY: u64 = 2_000;

#[derive(Clone)]
pub struct SkinImage {
    pub data: DynamicImage,
    pub slim: bool,
}

#[async_trait]
pub trait SkinProvider: Send + Sync {
    async fn fetch(&self, uuid: &str) -> Option<SkinImage>;
    async fn fetch_with_url(&self, uuid: &str, skin_url: &str, slim: bool) -> Option<SkinImage>;
    async fn fetch_face(&self, uuid: &str, size: u32) -> Option<Vec<u8>>;
}

pub struct LocalSkinProvider {
    http: Client,
    mojang: MojangClient,
    renderer: Arc<Renderer>,
    uuid_cache: Cache<String, Arc<SkinImage>>,
}

impl LocalSkinProvider {
    pub fn new() -> Option<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;

        let renderer = Renderer::new().ok()?;

        let uuid_cache = Cache::builder()
            .time_to_live(Duration::from_secs(CACHE_TTL_SECS))
            .max_capacity(CACHE_CAPACITY)
            .build();

        Some(Self {
            http,
            mojang: MojangClient::new(),
            renderer: Arc::new(renderer),
            uuid_cache,
        })
    }

    async fn download_skin(&self, skin_url: &str) -> Option<Skin> {
        let bytes = self.http
            .get(skin_url)
            .header("User-Agent", USER_AGENT)
            .send().await.ok()?
            .error_for_status().ok()?
            .bytes().await.ok()?;
        Skin::from_bytes(&bytes).ok()
    }

    async fn download_and_render(&self, uuid: &str, skin_url: &str, slim_override: Option<bool>) -> Option<SkinImage> {
        let mut skin = self.download_skin(skin_url).await?;
        if let Some(slim) = slim_override {
            skin.set_slim(slim);
        }

        let output = self.renderer
            .render(&skin, &Pose::standing(), OutputType::full_body(400, 600))
            .ok()?;

        let skin_image = SkinImage {
            data: DynamicImage::ImageRgba8(output.image),
            slim: skin.is_slim(),
        };

        self.uuid_cache
            .insert(uuid.to_string(), Arc::new(skin_image.clone()))
            .await;

        Some(skin_image)
    }
}


#[async_trait]
impl SkinProvider for LocalSkinProvider {
    async fn fetch(&self, uuid: &str) -> Option<SkinImage> {
        if let Some(cached) = self.uuid_cache.get(uuid).await {
            return Some((*cached).clone());
        }

        let profile = self.mojang.get_profile(uuid).await.ok()?;
        let skin_url = profile.skin_url?;
        self.download_and_render(uuid, &skin_url, Some(profile.slim)).await
    }

    async fn fetch_with_url(&self, uuid: &str, skin_url: &str, slim: bool) -> Option<SkinImage> {
        if let Some(cached) = self.uuid_cache.get(uuid).await {
            return Some((*cached).clone());
        }

        self.download_and_render(uuid, skin_url, Some(slim)).await
    }

    async fn fetch_face(&self, uuid: &str, size: u32) -> Option<Vec<u8>> {
        let profile = self.mojang.get_profile(uuid).await.ok()?;
        let skin_url = profile.skin_url?;
        let mut skin = self.download_skin(&skin_url).await?;
        skin.set_slim(profile.slim);

        let output = self.renderer
            .render(&skin, &Pose::standing(), OutputType::face(size))
            .ok()?;

        output.to_png_bytes().ok()
    }
}
