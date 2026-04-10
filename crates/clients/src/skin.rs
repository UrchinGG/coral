use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use image::{DynamicImage, RgbaImage};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use reqwest::Client;

use render::skin::{OutputType, Pose, Renderer, Skin};

use crate::MojangClient;

const USER_AGENT: &str = "Coral/1.0";
const CACHE_TTL_SECS: u64 = 10 * 60;

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
    redis: ConnectionManager,
}

impl LocalSkinProvider {
    pub fn new(redis: ConnectionManager) -> Option<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;

        let renderer = Renderer::new().ok()?;

        Some(Self {
            http,
            mojang: MojangClient::new(),
            renderer: Arc::new(renderer),
            redis,
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

        self.cache_set(uuid, &skin_image).await;
        Some(skin_image)
    }

    async fn cache_get(&self, uuid: &str) -> Option<SkinImage> {
        let key = format!("cache:skin:{uuid}");
        let data: Vec<u8> = self.redis.clone().get(&key).await.ok()?;
        if data.len() < 9 { return None; }

        let slim = data[0] != 0;
        let width = u32::from_le_bytes(data[1..5].try_into().ok()?);
        let height = u32::from_le_bytes(data[5..9].try_into().ok()?);
        let pixels = data[9..].to_vec();

        let image = RgbaImage::from_raw(width, height, pixels)?;
        Some(SkinImage { data: DynamicImage::ImageRgba8(image), slim })
    }

    async fn cache_set(&self, uuid: &str, skin: &SkinImage) {
        let key = format!("cache:skin:{uuid}");
        let rgba = skin.data.to_rgba8();
        let (width, height) = rgba.dimensions();

        let mut buf = Vec::with_capacity(9 + (width * height * 4) as usize);
        buf.push(skin.slim as u8);
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&height.to_le_bytes());
        buf.extend_from_slice(rgba.as_raw());

        let _: Result<(), _> = self.redis.clone().set_ex(&key, buf, CACHE_TTL_SECS).await;
    }
}


#[async_trait]
impl SkinProvider for LocalSkinProvider {
    async fn fetch(&self, uuid: &str) -> Option<SkinImage> {
        if let Some(cached) = self.cache_get(uuid).await {
            return Some(cached);
        }

        let profile = self.mojang.get_profile(uuid).await.ok()?;
        let skin_url = profile.skin_url?;
        self.download_and_render(uuid, &skin_url, Some(profile.slim)).await
    }

    async fn fetch_with_url(&self, uuid: &str, skin_url: &str, slim: bool) -> Option<SkinImage> {
        if let Some(cached) = self.cache_get(uuid).await {
            return Some(cached);
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
