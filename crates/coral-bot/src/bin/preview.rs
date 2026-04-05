use std::env;
use std::time::Duration;

use image::{DynamicImage, RgbaImage};
use render::init_canvas;
use render::skin::{OutputType, Pose, Renderer, Skin};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

use coral_bot::preview::PlayerData;


type RenderFn = fn(&PlayerData, &[String]) -> RgbaImage;


struct Card {
    name: &'static str,
    description: &'static str,
    needs_player: bool,
    render: RenderFn,
}


fn cards() -> Vec<Card> {
    vec![
        Card {
            name: "bedwars",
            description: "Bedwars overall stats card",
            needs_player: true,
            render: coral_bot::commands::stats::bedwars::cards::overall::preview,
        },
        Card {
            name: "bedwars_session",
            description: "Bedwars session stats card",
            needs_player: true,
            render: coral_bot::commands::stats::bedwars::cards::session::preview,
        },
        Card {
            name: "duels",
            description: "Duels overall stats card [view]",
            needs_player: true,
            render: coral_bot::commands::stats::duels::cards::overall::preview,
        },
        Card {
            name: "duels_session",
            description: "Duels session stats card [view]",
            needs_player: true,
            render: coral_bot::commands::stats::duels::cards::session::preview,
        },
        Card {
            name: "prestiges",
            description: "Bedwars prestige grid",
            needs_player: false,
            render: |_, _| coral_bot::commands::stats::bedwars::cards::prestiges::render_prestiges(),
        },
    ]
}


fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find project root");
    dotenvy::from_path(project_root.join(".env")).ok();

    init_canvas();

    let args: Vec<String> = env::args().skip(1).collect();
    let registry = cards();

    let card_name = args.first().map(String::as_str).unwrap_or("help");
    let Some(card) = registry.iter().find(|c| c.name == card_name) else {
        println!("Usage: preview <card> [player] [args...]\n");
        println!("Available cards:");
        for c in &registry {
            let player = if c.needs_player { " <player>" } else { "" };
            println!("  {:<20} {}", format!("{}{player}", c.name), c.description);
        }
        return;
    };

    let (player_data, extra_args) = if card.needs_player {
        let player_name = args.get(1).map(String::as_str).unwrap_or("WarOG");
        let api_key = env::var("HYPIXEL_API_KEY").expect("HYPIXEL_API_KEY not set");
        let http = Client::builder().timeout(Duration::from_secs(10)).build().unwrap();
        (fetch_player_data(&http, &api_key, player_name), args.get(2..).unwrap_or_default().to_vec())
    } else {
        (PlayerData::empty(), args[1..].to_vec())
    };

    let image = (card.render)(&player_data, &extra_args);

    let output = args.iter().find_map(|a| a.strip_prefix("--out="))
        .unwrap_or("preview.png");
    image.save(output).expect("Failed to save image");
    println!("Saved {} to {output}", card.name);
}


fn fetch_player_data(http: &Client, api_key: &str, name: &str) -> PlayerData {
    let resp: Value = http.get(format!("https://api.mojang.com/users/profiles/minecraft/{name}")).send().unwrap().json().unwrap();
    let uuid = resp["id"].as_str().expect("No UUID").to_string();
    let username = resp["name"].as_str().unwrap_or(name).to_string();
    println!("Player: {username} ({uuid})");

    let skin = fetch_skin(http, &uuid);
    let hypixel: Value = http.get(format!("https://api.hypixel.net/v2/player?uuid={uuid}")).header("API-Key", api_key).send().unwrap().json().unwrap();
    let guild: Value = http.get(format!("https://api.hypixel.net/v2/guild?player={uuid}")).header("API-Key", api_key).send().unwrap().json().unwrap();

    PlayerData {
        uuid,
        username,
        skin,
        hypixel: hypixel.get("player").cloned().unwrap_or(Value::Null),
        guild,
    }
}


fn fetch_skin(http: &Client, uuid: &str) -> Option<DynamicImage> {
    let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}");
    let profile: MojangProfile = http.get(&url).send().ok()?.json().ok()?;
    let prop = profile.properties.first()?;
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &prop.value).ok()?;
    let payload: TexturesPayload = serde_json::from_slice(&decoded).ok()?;
    let skin_url = payload.textures.skin?.url;
    let skin_bytes = http.get(&skin_url).send().ok()?.bytes().ok()?;
    let skin = Skin::from_bytes(&skin_bytes).ok()?;
    let renderer = Renderer::new().ok()?;
    let output = renderer.render(&skin, &Pose::standing(), OutputType::full_body(400, 600)).ok()?;
    Some(DynamicImage::ImageRgba8(output.image))
}


#[derive(Deserialize)]
struct MojangProfile {
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
