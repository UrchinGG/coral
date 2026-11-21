use replay_model::{EventKind, Replay};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
export type Camera = "map" | "follow" | "pov" | "fly";

export interface Adapter {
    name: string;
    backend: "webgpu" | "webgl";
    software: boolean;
    tier: "low" | "mid" | "high";
}

export interface ReplayInfo {
    durationMs: number;
    startMs: number;
    game: string | null;
    mode: string | null;
    map: string | null;
    date: string | null;
    time: string | null;
    players: ReplayPlayer[];
    chat: ChatLine[];
    skins: string[];
}

export interface ReplayPlayer {
    index: number;
    username: string;
    teamColor: string;
    skinHash: string | null;
    skinOwner: string | null;
}

export interface ChatLine {
    t: number;
    text: string;
    segments: ChatSegment[];
}

export interface ChatSegment {
    text: string;
    color: string;
    bold: boolean;
    italic: boolean;
    underlined: boolean;
    strikethrough: boolean;
}
"#;

pub fn json(replay: &Replay, skins: &[String]) -> String {
    let info = &replay.info;
    let players: Vec<Value> = replay
        .players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            json!({
                "index": index,
                "username": player.username,
                "teamColor": hex(player.team_color),
                "skinHash": player.skin_hash,
                "skinOwner": player.skin_owner,
            })
        })
        .collect();
    let chat: Vec<Value> = replay
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Chat { text, segments } => Some(json!({
                "t": event.t,
                "text": text,
                "segments": segments
                    .iter()
                    .map(|segment| json!({
                        "text": segment.text,
                        "color": hex(segment.style.color),
                        "bold": segment.style.bold,
                        "italic": segment.style.italic,
                        "underlined": segment.style.underlined,
                        "strikethrough": segment.style.strikethrough,
                    }))
                    .collect::<Vec<_>>(),
            })),
            _ => None,
        })
        .collect();
    json!({
        "durationMs": info.duration,
        "startMs": info.stable_start,
        "game": info.game,
        "mode": info.mode,
        "map": info.map,
        "date": info.date,
        "time": info.time,
        "players": players,
        "chat": chat,
        "skins": skins,
    })
    .to_string()
}

fn hex(rgb: u32) -> String {
    format!("#{rgb:06x}")
}
