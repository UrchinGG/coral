use std::env;
use std::fs;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use hypixel::{
    DuelsView, GuildInfo, Mode, WinstreakHistory, extract_bedwars_stats, extract_duels_stats,
};
use image::DynamicImage;
use render::skin::{OutputType, Pose, Renderer, Skin};
use render::{
    SessionType, init_canvas, render_bedwars, render_duels, render_duels_session, render_session,
};
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
    let manifest_path = std::path::Path::new(manifest_dir);
    let parent = manifest_path
        .parent()
        .expect("Failed to find render parent");
    let project_root = if parent.join("Cargo.toml").exists() {
        parent.to_path_buf()
    } else {
        parent
            .parent()
            .expect("Failed to find project root")
            .to_path_buf()
    };
    dotenvy::from_path(project_root.join(".env")).ok();

    init_canvas();

    let mut args = env::args().skip(1);
    let preview_kind = args.next().unwrap_or_else(|| "bedwars".to_string());
    let player_name = args.next().unwrap_or_else(|| "WarOG".to_string());
    let duels_view = args
        .next()
        .and_then(|value| DuelsView::from_slug(&value))
        .unwrap_or(DuelsView::Overall);

    if player_name == "sample" || env::var("HYPIXEL_API_KEY").is_err() {
        let sample_path = project_root.join("coral-web/public/player.json");
        let fallback_path = project_root.join("player.json");
        let sample_file = if sample_path.exists() {
            sample_path
        } else {
            fallback_path
        };
        let sample: Value = serde_json::from_str(
            &fs::read_to_string(&sample_file).expect("Failed to read sample player"),
        )
        .expect("Failed to parse sample player");
        let player = sample
            .get("player")
            .expect("Sample player missing `player` key");
        let username = player
            .get("displayname")
            .and_then(|v| v.as_str())
            .unwrap_or("SamplePlayer");
        render_preview(&preview_kind, username, player, None, None, duels_view);
        return;
    }

    let api_key = env::var("HYPIXEL_API_KEY").expect("HYPIXEL_API_KEY not set");

    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to create HTTP client");

    let mojang_url = format!(
        "https://api.mojang.com/users/profiles/minecraft/{}",
        player_name
    );
    let mojang_resp: Value = http.get(&mojang_url).send().unwrap().json().unwrap();
    let uuid = mojang_resp["id"].as_str().expect("No UUID found");
    let username = mojang_resp["name"].as_str().unwrap_or(&player_name);
    println!("Found player: {} ({})", username, uuid);

    let profile_url = format!(
        "https://sessionserver.mojang.com/session/minecraft/profile/{}",
        uuid
    );
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
        .header("API-Key", &api_key)
        .send()
        .unwrap()
        .json()
        .unwrap();
    let player_data = hypixel_resp.get("player").expect("No player data");

    let guild_url = format!("https://api.hypixel.net/v2/guild?player={}", uuid);
    let guild_resp: Value = http
        .get(&guild_url)
        .header("API-Key", &api_key)
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

        let rank = member
            .get("rank")
            .and_then(|r| r.as_str())
            .map(String::from);
        let joined = member.get("joined").and_then(|j| j.as_i64());
        let weekly_gexp: Option<u64> = member.get("expHistory").and_then(|h| {
            h.as_object()
                .map(|obj| obj.values().filter_map(|v| v.as_u64()).sum())
        });

        Some(GuildInfo {
            name: Some(name),
            tag,
            tag_color,
            rank,
            joined,
            weekly_gexp,
        })
    });

    render_preview(
        &preview_kind,
        username,
        player_data,
        guild_info,
        skin_image,
        duels_view,
    );
}

fn render_preview(
    preview_kind: &str,
    username: &str,
    player_data: &Value,
    guild_info: Option<GuildInfo>,
    skin_image: Option<DynamicImage>,
    duels_view: DuelsView,
) {
    match preview_kind {
        "bedwars" => {
            let stats = extract_bedwars_stats(username, player_data, guild_info)
                .expect("Failed to extract bedwars stats");
            let image = render_bedwars(
                &stats,
                &[Mode::Overall],
                skin_image.as_ref(),
                &WinstreakHistory {
                    streaks: Vec::new(),
                },
                &[],
            );
            image
                .save("preview-bedwars.png")
                .expect("Failed to save image");
            println!("Saved to preview-bedwars.png");
        }
        "session-bedwars" => {
            let current = extract_bedwars_stats(username, player_data, guild_info)
                .expect("Failed to extract bedwars stats");
            let previous = synthetic_previous_bedwars(&current);
            let image = render_session(
                &current,
                &previous,
                SessionType::Daily,
                Utc::now() - ChronoDuration::days(1),
                None,
                &[Mode::Overall],
                skin_image.as_ref(),
                &[],
            );
            image
                .save("preview-session-bedwars.png")
                .expect("Failed to save image");
            println!("Saved to preview-session-bedwars.png");
        }
        "duels" => {
            let stats = extract_duels_stats(username, player_data, guild_info)
                .expect("Failed to extract duels stats");
            let image = render_duels(&stats, duels_view, skin_image.as_ref(), &WinstreakHistory { streaks: vec![] }, &[]);
            let output = format!("preview-duels-{}.png", duels_view.slug());
            image
                .save(&output)
                .expect("Failed to save image");
            println!("Saved to {output}");
        }
        "session-duels" => {
            let current = extract_duels_stats(username, player_data, guild_info)
                .expect("Failed to extract duels stats");
            let previous = synthetic_previous_duels(&current);
            let image = render_duels_session(
                &current,
                &previous,
                SessionType::Daily,
                Utc::now() - ChronoDuration::days(1),
                None,
                duels_view,
                skin_image.as_ref(),
                &WinstreakHistory { streaks: vec![] },
                &[],
            );
            let output = format!("preview-session-duels-{}.png", duels_view.slug());
            image
                .save(&output)
                .expect("Failed to save image");
            println!("Saved to {output}");
        }
        other => panic!("Unknown preview kind: {other}"),
    }
}

fn synthetic_previous_bedwars(
    current: &hypixel::BedwarsPlayerStats,
) -> hypixel::BedwarsPlayerStats {
    let mut previous = current.clone();
    previous.experience = previous.experience.saturating_sub(7_500);
    previous.games_played = previous.games_played.saturating_sub(12);
    previous.overall.wins = previous.overall.wins.saturating_sub(7);
    previous.overall.losses = previous.overall.losses.saturating_sub(5);
    previous.overall.kills = previous.overall.kills.saturating_sub(24);
    previous.overall.deaths = previous.overall.deaths.saturating_sub(10);
    previous.overall.final_kills = previous.overall.final_kills.saturating_sub(15);
    previous.overall.final_deaths = previous.overall.final_deaths.saturating_sub(4);
    previous.overall.beds_broken = previous.overall.beds_broken.saturating_sub(6);
    previous.overall.beds_lost = previous.overall.beds_lost.saturating_sub(3);
    previous.solos.wins = previous.solos.wins.saturating_sub(2);
    previous.solos.losses = previous.solos.losses.saturating_sub(1);
    previous.doubles.wins = previous.doubles.wins.saturating_sub(3);
    previous.doubles.losses = previous.doubles.losses.saturating_sub(2);
    previous.threes.wins = previous.threes.wins.saturating_sub(1);
    previous.fours.wins = previous.fours.wins.saturating_sub(1);
    previous
}

fn synthetic_previous_duels(current: &hypixel::DuelsStats) -> hypixel::DuelsStats {
    let mut previous = current.clone();
    previous.overview.wins = previous.overview.wins.saturating_sub(14);
    previous.overview.losses = previous.overview.losses.saturating_sub(9);
    previous.overview.kills = previous.overview.kills.saturating_sub(38);
    previous.overview.deaths = previous.overview.deaths.saturating_sub(16);
    previous.overview.melee_hits = previous.overview.melee_hits.saturating_sub(60);
    previous.overview.melee_swings = previous.overview.melee_swings.saturating_sub(95);
    previous.overview.bow_hits = previous.overview.bow_hits.saturating_sub(12);
    previous.overview.bow_shots = previous.overview.bow_shots.saturating_sub(24);

    for (_, mode) in &mut previous.modes {
        mode.wins = mode.wins.saturating_sub(mode.wins.min(3));
        mode.losses = mode.losses.saturating_sub(mode.losses.min(2));
        mode.kills = mode.kills.saturating_sub(mode.kills.min(8));
        mode.deaths = mode.deaths.saturating_sub(mode.deaths.min(4));
        mode.bridge_kills = mode.bridge_kills.saturating_sub(mode.bridge_kills.min(5));
        mode.bridge_deaths = mode.bridge_deaths.saturating_sub(mode.bridge_deaths.min(2));
        mode.melee_hits = mode.melee_hits.saturating_sub(mode.melee_hits.min(10));
        mode.melee_swings = mode.melee_swings.saturating_sub(mode.melee_swings.min(16));
        mode.bow_hits = mode.bow_hits.saturating_sub(mode.bow_hits.min(3));
        mode.bow_shots = mode.bow_shots.saturating_sub(mode.bow_shots.min(5));
        mode.goals = mode.goals.saturating_sub(mode.goals.min(2));
    }

    previous
}
