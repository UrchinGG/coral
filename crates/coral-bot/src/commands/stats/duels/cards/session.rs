use std::collections::HashMap;

use chrono::{DateTime, Utc};
use image::{DynamicImage, RgbaImage};
use mctext::{MCText, NamedColor};

use hypixel::{DuelsBreakdownEntry, DuelsStats, DuelsView, DuelsViewStats, WinstreakHistory};

use render::canvas::{Align, CANVAS_BACKGROUND, Canvas, TextBox};
use render::cards::{SessionType, TagIcon};

use super::overall::{
    BreakdownBox, DivisionSection, DuelsGuildBox, HeaderSection, SkinSection, StatsSection,
    extras_box,
};


const CANVAS_WIDTH: u32 = 800;
const CANVAS_HEIGHT: u32 = 600;
const COL_WIDTH: u32 = 256;
const LEVEL_Y: u32 = 57;
const MAIN_ROW_Y: u32 = 116;
const STATS_BOX_HEIGHT: u32 = 176;
const SECOND_ROW_Y: u32 = MAIN_ROW_Y + STATS_BOX_HEIGHT + 16;
const BOTTOM_ROW_Y: u32 = 500;
const BOTTOM_BOX_HEIGHT: u32 = 100;
const BOX_CORNER_RADIUS: u32 = 18;


fn col_x(col: u32) -> u32 {
    match col {
        0 => 0,
        1 => 272,
        2 => 544,
        _ => 0,
    }
}


pub fn render_duels_session(
    current: &DuelsStats,
    previous: &DuelsStats,
    session_type: SessionType,
    started: DateTime<Utc>,
    ended: Option<DateTime<Utc>>,
    view: DuelsView,
    skin: Option<&DynamicImage>,
    winstreaks: &WinstreakHistory,
    tags: &[TagIcon],
) -> RgbaImage {
    let current_view = current
        .view_stats(view)
        .unwrap_or_else(|| current.view_stats(current.default_view()).expect("duels current"));
    let previous_view = previous
        .view_stats(view)
        .unwrap_or_else(|| previous.view_stats(previous.default_view()).expect("duels previous"));
    let delta = DuelsDelta::from_views(&current_view, &previous_view);

    let overall = current.view_stats(DuelsView::Overall);
    let is_overall = view == DuelsView::Overall;
    let session_breakdown = if is_overall {
        BreakdownBox::overall(&delta.breakdown)
    } else if let Some(ref ov) = overall {
        BreakdownBox::mode_share(&delta.breakdown, ov, &current_view)
    } else {
        BreakdownBox::overall(&delta.breakdown)
    };

    Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT)
        .background(CANVAS_BACKGROUND)
        .draw(
            0, 0,
            &HeaderSection::new(&current.display_name, current.rank_prefix.as_deref(), &current.guild, tags),
        )
        .draw(
            0, LEVEL_Y as i32,
            &DivisionSection {
                division: current_view.division,
                track: current_view.track,
                wins: current_view.wins,
                session_wins: Some(delta.summary.wins),
            },
        )
        .draw(
            col_x(0) as i32, MAIN_ROW_Y as i32,
            &SkinSection::new(skin, current.network_level, &format!("{} Session", current_view.title)),
        )
        .draw(col_x(1) as i32, MAIN_ROW_Y as i32, &StatsSection::new(&delta.summary))
        .draw(col_x(1) as i32, SECOND_ROW_Y as i32, &session_breakdown)
        .draw(
            col_x(2) as i32, SECOND_ROW_Y as i32,
            &super::overall::DuelsWinstreaksBox { winstreaks, current_ws: current_view.current_winstreak.value() },
        )
        .draw(col_x(0) as i32, BOTTOM_ROW_Y as i32, &duels_session_box(&session_type, started, ended))
        .draw(col_x(1) as i32, BOTTOM_ROW_Y as i32, &DuelsGuildBox::new(&current.guild))
        .draw(col_x(2) as i32, BOTTOM_ROW_Y as i32, &extras_box(current))
        .build()
}


#[derive(Clone)]
pub struct DuelsDelta {
    pub summary: DuelsViewStats,
    pub breakdown: Vec<DuelsBreakdownEntry>,
}


impl DuelsDelta {
    pub fn from_views(current: &DuelsViewStats, previous: &DuelsViewStats) -> Self {
        let previous_by_label: HashMap<&str, &DuelsBreakdownEntry> = previous
            .breakdown
            .iter()
            .map(|entry| (entry.label.as_str(), entry))
            .collect();

        let breakdown = current
            .breakdown
            .iter()
            .map(|entry| {
                let previous_entry = previous_by_label.get(entry.label.as_str()).copied();
                let previous_wins = previous_entry.map(|value| value.wins).unwrap_or(0);
                let previous_losses = previous_entry.map(|value| value.losses).unwrap_or(0);
                let previous_kills = previous_entry.map(|value| value.kills).unwrap_or(0);
                let previous_deaths = previous_entry.map(|value| value.deaths).unwrap_or(0);
                let previous_melee_hits = previous_entry.map(|value| value.melee_hits).unwrap_or(0);
                let previous_melee_swings = previous_entry.map(|value| value.melee_swings).unwrap_or(0);
                let previous_bow_hits = previous_entry.map(|value| value.bow_hits).unwrap_or(0);
                let previous_bow_shots = previous_entry.map(|value| value.bow_shots).unwrap_or(0);
                let previous_goals = previous_entry.map(|value| value.goals).unwrap_or(0);

                DuelsBreakdownEntry {
                    label: entry.label.clone(),
                    division: entry.division,
                    wins: entry.wins.saturating_sub(previous_wins),
                    losses: entry.losses.saturating_sub(previous_losses),
                    kills: entry.kills.saturating_sub(previous_kills),
                    deaths: entry.deaths.saturating_sub(previous_deaths),
                    melee_hits: entry.melee_hits.saturating_sub(previous_melee_hits),
                    melee_swings: entry.melee_swings.saturating_sub(previous_melee_swings),
                    bow_hits: entry.bow_hits.saturating_sub(previous_bow_hits),
                    bow_shots: entry.bow_shots.saturating_sub(previous_bow_shots),
                    goals: entry.goals.saturating_sub(previous_goals),
                    current_winstreak: entry.current_winstreak,
                    best_winstreak: entry.best_winstreak,
                }
            })
            .filter(|entry| entry.wins > 0 || entry.losses > 0 || entry.kills > 0 || entry.deaths > 0)
            .collect();

        let summary = DuelsViewStats {
            view: current.view,
            title: current.title.clone(),
            division: current.division,
            wins: current.wins.saturating_sub(previous.wins),
            losses: current.losses.saturating_sub(previous.losses),
            kills: current.kills.saturating_sub(previous.kills),
            deaths: current.deaths.saturating_sub(previous.deaths),
            melee_hits: current.melee_hits.saturating_sub(previous.melee_hits),
            melee_swings: current.melee_swings.saturating_sub(previous.melee_swings),
            bow_hits: current.bow_hits.saturating_sub(previous.bow_hits),
            bow_shots: current.bow_shots.saturating_sub(previous.bow_shots),
            goals: current.goals.saturating_sub(previous.goals),
            current_winstreak: current.current_winstreak,
            best_winstreak: current.best_winstreak,
            show_goals: current.show_goals,
            breakdown_title: current.breakdown_title,
            breakdown: Vec::new(),
            track: current.track,
        };

        Self { summary, breakdown }
    }
}


fn duels_session_box(
    session_type: &SessionType,
    started: DateTime<Utc>,
    ended: Option<DateTime<Utc>>,
) -> TextBox {
    let finished = ended.unwrap_or_else(Utc::now);
    let duration = finished.signed_duration_since(started);
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;

    let duration_str = match (days, hours, minutes) {
        (d, h, _) if d > 0 && h > 0 => format!("{d}d {h}h"),
        (d, _, _) if d > 0 => format!("{d}d"),
        (_, h, m) if h > 0 && m > 0 => format!("{h}h {m}m"),
        (_, h, _) if h > 0 => format!("{h}h"),
        (_, _, m) => format!("{m}m"),
    };

    let start_str = started.format("%m/%d/%y %H:%M").to_string();

    TextBox::new()
        .width(COL_WIDTH).height(BOTTOM_BOX_HEIGHT).corner_radius(BOX_CORNER_RADIUS)
        .padding(12, 12).scale(1.5).line_spacing(0.0)
        .align_x(Align::Center).align_y(Align::Spread)
        .push(MCText::new().span("Session: ").color(NamedColor::Gray).then(session_type.display_name()).color(NamedColor::White).build())
        .push(MCText::new().span("Start: ").color(NamedColor::Gray).then(&start_str).color(NamedColor::White).build())
        .push(MCText::new().span("Duration: ").color(NamedColor::Gray).then(&duration_str).color(NamedColor::Green).build())
}


pub fn preview(data: &crate::preview::PlayerData, args: &[String]) -> RgbaImage {
    let view = args.first()
        .and_then(|v| DuelsView::from_slug(v))
        .unwrap_or(DuelsView::Overall);
    let stats = hypixel::extract_duels_stats(&data.username, &data.hypixel, data.guild_info())
        .expect("No Duels stats");
    let ws = WinstreakHistory { streaks: vec![] };
    render_duels_session(&stats, &stats, SessionType::Daily, chrono::Utc::now(), None, view, data.skin.as_ref(), &ws, &[])
}
