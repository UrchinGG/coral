use std::env;
use std::time::Duration;

use hypixel::{GuildInfo, Mode, WinstreakHistory, extract_bedwars_stats};
use image::DynamicImage;
use render::skin::{OutputType, Pose, Renderer, Skin};
use render::{init_canvas, render_bedwars};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct MojangProfile {
    id: String,
    name: String,
    properties: Vec<MojangProperty>,
}


#[derive(Deserialize)]
struct MojangProperty {
    value: String,
}


#[derive(Deserialize)]
struct TexturesPayload {
    textures: Textures,
}


#[derive(Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<SkinTexture>,
}


#[derive(Deserialize)]
struct SkinTexture {
    url: String,
}


fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find project root");
    dotenvy::from_path(project_root.join(".env")).ok();

    init_canvas();

    let api_key = env::var("HYPIXEL_API_KEY").expect("HYPIXEL_API_KEY not set");
    let player_name = env::args().nth(1).unwrap_or_else(|| "WarOG".to_string());

    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client");

    let mojang_url = format!("https://api.mojang.com/users/profiles/minecraft/{}", player_name);
    let mojang_resp: Value = http.get(&mojang_url).send().unwrap().json().unwrap();
    let uuid = mojang_resp["id"].as_str().expect("No UUID found");
    let username = mojang_resp["name"].as_str().unwrap_or(&player_name);
    println!("Found player: {} ({})", username, uuid);

    let profile_url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", uuid);
    let profile: MojangProfile = http.get(&profile_url).send().unwrap().json().unwrap();

    let skin_image = profile.properties.first().and_then(|prop| {
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &prop.value)
                .unwrap();
        let payload: TexturesPayload = serde_json::from_slice(&decoded).unwrap();
        let skin_texture = payload.textures.skin?;
        println!("Fetching skin from: {}", skin_texture.url);
        let skin_bytes = http.get(&skin_texture.url).send().unwrap().bytes().unwrap();
        let skin = Skin::from_bytes(&skin_bytes).expect("Failed to parse skin");
        let renderer = Renderer::new().expect("Failed to create renderer");
        let output = renderer
            .render(&skin, &Pose::standing(), OutputType::full_body(400, 600))
            .expect("Failed to render skin");
        Some(DynamicImage::ImageRgba8(output.image))
    });

    let hypixel_url = format!("https://api.hypixel.net/v2/player?uuid={}", uuid);
    let hypixel_resp: Value = http
        .get(&hypixel_url)
        .header("API-Key", api_key)
        .send()
        .unwrap()
        .json()
        .unwrap();
    let player_data = hypixel_resp.get("player").expect("No player data");

    let guild_url = format!("https://api.hypixel.net/v2/guild?player={}", uuid);
    let guild_resp: Value = http
        .get(&guild_url)
        .header("API-Key", api_key)
        .send()
        .unwrap()
        .json()
        .unwrap();

    let guild_info: Option<GuildInfo> = guild_resp.get("guild").and_then(|g| {
        let name = g.get("name")?.as_str()?.to_string();
        let tag = g.get("tag").and_then(|t| t.as_str()).map(String::from);
        let tag_color = g.get("tagColor").and_then(|c| c.as_str()).map(String::from);

        let members = g.get("members")?.as_array()?;
        let member = members.iter().find(|m| {
            m.get("uuid")
                .and_then(|u| u.as_str())
                .map(|u| u.replace("-", "").to_lowercase())
                == Some(uuid.to_lowercase())
        })?;

        let rank = member.get("rank").and_then(|r| r.as_str()).map(String::from);
        let joined = member.get("joined").and_then(|j| j.as_i64());
        let weekly_gexp: Option<u64> = member
            .get("expHistory")
            .and_then(|h| h.as_object().map(|obj| obj.values().filter_map(|v| v.as_u64()).sum()));

        Some(GuildInfo { name: Some(name), tag, tag_color, rank, joined, weekly_gexp })
    });

    let stats =
        extract_bedwars_stats(username, player_data, guild_info).expect("Failed to extract stats");
    println!("Level: {} | FKDR: {:.2}", stats.level, stats.overall.fkdr());

    let image = render_bedwars(
        &stats,
        Mode::Overall,
        skin_image.as_ref(),
        &WinstreakHistory { streaks: Vec::new() },
        &[],
    );
    image.save("preview.png").expect("Failed to save image");
    println!("Saved to preview.png");
}
