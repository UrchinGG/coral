use std::collections::HashMap;

use chrono::{DateTime, Utc};
use image::{DynamicImage, RgbaImage};
use mctext::{MCText, NamedColor};

use hypixel::{
    DivisionTrack, DuelsBreakdownEntry, DuelsDivision, DuelsStats, DuelsView, DuelsViewStats,
    GuildInfo, StreakSource, WinstreakHistory, color_code, division_progress,
    next_division, win_progress,
};

use crate::canvas::{
    Align, BOX_BACKGROUND, CANVAS_BACKGROUND, Canvas, DrawContext, Image, RoundedRect, Shape,
    TextBlock, TextBox,
};

use super::bedwars::TagIcon;
use super::common::{BAR_COLOR, color_name_to_named, colors, draw_progress_bar, format_number, format_percent, format_ratio, format_timestamp, stat_line};
use super::session::SessionType;


const BOX_CORNER_RADIUS: u32 = 18;
const CANVAS_WIDTH: u32 = 800;
const CANVAS_HEIGHT: u32 = 600;
const COL_WIDTH: u32 = 256;
const HEADER_HEIGHT: u32 = 100;
const LEVEL_Y: u32 = 57;
const MAIN_ROW_Y: u32 = 116;
const STATS_BOX_HEIGHT: u32 = 176;
const STATS_BOX_WIDTH: u32 = 528;
const SKIN_BOX_HEIGHT: u32 = 368;
const SECOND_ROW_Y: u32 = MAIN_ROW_Y + STATS_BOX_HEIGHT + 16;
const SECOND_ROW_HEIGHT: u32 = 176;
const BOTTOM_ROW_Y: u32 = 500;
const BOTTOM_BOX_HEIGHT: u32 = 100;
const LEVEL_SCALE: f32 = 2.75;
const LEVEL_PADDING: u32 = 20;
const SKIN_PADDING: u32 = 12;
const MAX_DISPLAYED_STREAKS: usize = 5;


fn col_x(col: u32) -> u32 {
    match col {
        0 => 0,
        1 => 272,
        2 => 544,
        _ => 0,
    }
}


pub fn render_duels(
    stats: &DuelsStats,
    view: DuelsView,
    skin: Option<&DynamicImage>,
    winstreaks: &WinstreakHistory,
    tags: &[TagIcon],
) -> RgbaImage {
    let selected = stats
        .view_stats(view)
        .unwrap_or_else(|| stats.view_stats(stats.default_view()).expect("duels default view"));
    let overall = stats.view_stats(DuelsView::Overall);
    let is_overall = view == DuelsView::Overall;

    let breakdown = if is_overall {
        BreakdownBox::overall(&selected.breakdown)
    } else if let Some(ref ov) = overall {
        BreakdownBox::mode_share(&selected.breakdown, ov, &selected)
    } else {
        BreakdownBox::overall(&selected.breakdown)
    };

    Canvas::new(CANVAS_WIDTH, CANVAS_HEIGHT)
        .background(CANVAS_BACKGROUND)
        .draw(
            0,
            0,
            &HeaderSection::new(
                &stats.display_name,
                stats.rank_prefix.as_deref(),
                &stats.guild,
                tags,
            ),
        )
        .draw(
            0,
            LEVEL_Y as i32,
            &DivisionSection {
                division: selected.division,
                track: selected.track,
                wins: selected.wins,
                session_wins: None,
            },
        )
        .draw(
            col_x(0) as i32,
            MAIN_ROW_Y as i32,
            &SkinSection::new(skin, stats.network_level, &selected.title),
        )
        .draw(
            col_x(1) as i32,
            MAIN_ROW_Y as i32,
            &StatsSection::new(&selected, false),
        )
        .draw(col_x(1) as i32, SECOND_ROW_Y as i32, &breakdown)
        .draw(
            col_x(2) as i32,
            SECOND_ROW_Y as i32,
            &DuelsWinstreaksBox {
                winstreaks,
                current_ws: selected.current_winstreak.value(),
            },
        )
        .draw(
            col_x(0) as i32,
            BOTTOM_ROW_Y as i32,
            &status_box(stats.first_login),
        )
        .draw(
            col_x(1) as i32,
            BOTTOM_ROW_Y as i32,
            &DuelsGuildBox::new(&stats.guild),
        )
        .draw(
            col_x(2) as i32,
            BOTTOM_ROW_Y as i32,
            &view_box(&selected),
        )
        .build()
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
            0,
            0,
            &HeaderSection::new(
                &current.display_name,
                current.rank_prefix.as_deref(),
                &current.guild,
                tags,
            ),
        )
        .draw(
            0,
            LEVEL_Y as i32,
            &DivisionSection {
                division: current_view.division,
                track: current_view.track,
                wins: current_view.wins,
                session_wins: Some(delta.summary.wins),
            },
        )
        .draw(
            col_x(0) as i32,
            MAIN_ROW_Y as i32,
            &SkinSection::new(skin, current.network_level, &format!("{} Session", current_view.title)),
        )
        .draw(
            col_x(1) as i32,
            MAIN_ROW_Y as i32,
            &StatsSection::new(&delta.summary, true),
        )
        .draw(col_x(1) as i32, SECOND_ROW_Y as i32, &session_breakdown)
        .draw(
            col_x(2) as i32,
            SECOND_ROW_Y as i32,
            &DuelsWinstreaksBox {
                winstreaks,
                current_ws: current_view.current_winstreak.value(),
            },
        )
        .draw(
            col_x(0) as i32,
            BOTTOM_ROW_Y as i32,
            &duels_session_box(&session_type, started, ended),
        )
        .draw(
            col_x(1) as i32,
            BOTTOM_ROW_Y as i32,
            &DuelsGuildBox::new(&current.guild),
        )
        .draw(
            col_x(2) as i32,
            BOTTOM_ROW_Y as i32,
            &view_box(&current_view),
        )
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
                let previous_melee_swings = previous_entry
                    .map(|value| value.melee_swings)
                    .unwrap_or(0);
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


struct HeaderSection<'a> {
    display_name: &'a str,
    rank_prefix: Option<&'a str>,
    guild: &'a GuildInfo,
    tags: &'a [TagIcon],
}


impl<'a> HeaderSection<'a> {
    fn new(
        display_name: &'a str,
        rank_prefix: Option<&'a str>,
        guild: &'a GuildInfo,
        tags: &'a [TagIcon],
    ) -> Self {
        Self {
            display_name,
            rank_prefix,
            guild,
            tags,
        }
    }

    fn display_name_text(&self) -> MCText {
        let prefix = self.rank_prefix.unwrap_or("§7");
        let guild_tag = match (&self.guild.tag, &self.guild.tag_color) {
            (Some(tag), Some(color)) => format!(" {}[{}]", color_code(color), tag),
            (Some(tag), None) => format!(" §7[{}]", tag),
            _ => String::new(),
        };
        MCText::parse(&format!("{}{}{}", prefix, self.display_name, guild_tag))
    }
}


impl Shape for HeaderSection<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        RoundedRect::new(CANVAS_WIDTH, HEADER_HEIGHT)
            .corner_radius(BOX_CORNER_RADIUS)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        let name_text = self.display_name_text();
        let name_scale = 2.75;
        let name_font = name_scale * 16.0;
        let (cw, ch) = ctx.buffer.dimensions();
        let (name_w, _) = ctx.renderer.measure(&name_text, name_font);

        ctx.renderer.draw(
            ctx.buffer.as_mut(),
            cw,
            ch,
            (ctx.x + 20) as f32,
            (ctx.y + 13) as f32,
            &name_text,
            name_font,
            true,
        );

        if !self.tags.is_empty() {
            let icon_size = (name_scale * 12.0) as u32;
            let icon_gap = 4;
            let mut icon_x = 20.0 + name_w + 8.0;
            let icon_y = 13.0 + (name_font - icon_size as f32) / 2.0;
            for (icon_name, color) in self.tags {
                if let Some(icon) = crate::icons::tag_icon(icon_name, icon_size, *color) {
                    Image::new(&icon).draw(&mut ctx.at(icon_x as i32, icon_y as i32));
                    icon_x += icon_size as f32 + icon_gap as f32;
                }
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        (CANVAS_WIDTH, HEADER_HEIGHT)
    }
}


struct DivisionSection {
    division: (DuelsDivision, u32),
    track: DivisionTrack,
    wins: u64,
    session_wins: Option<u64>,
}


impl DivisionSection {
    fn current_text(&self) -> MCText {
        let (div, level) = self.division;
        let color = division_color(div.color_name);
        if div.name == "None" || level == 0 {
            MCText::new().span("-").color(NamedColor::Gray).build()
        } else {
            let mut span = MCText::new().span(&format!("[{}]", roman(level))).color(color);
            if div.bold { span = span.bold(); }
            span.build()
        }
    }

    fn right_text(&self) -> MCText {
        if let Some(session_wins) = self.session_wins {
            let color = division_color(self.division.0.color_name);
            MCText::new().span(&format!("+{} W", format_number(session_wins))).color(color).build()
        } else {
            let (div, level) = self.division;
            match next_division(div, level) {
                Some((next_div, next_level)) => {
                    let color = division_color(next_div.color_name);
                    let mut span = MCText::new().span(&format!("[{}]", roman(next_level))).color(color);
                    if next_div.bold { span = span.bold(); }
                    span.build()
                }
                None => self.current_text(),
            }
        }
    }

    fn progress_bar_text(&self) -> MCText {
        let progress = division_progress(self.wins, self.track);
        let filled = (progress * 25.0).round() as usize;
        MCText::new()
            .span("[").color(NamedColor::DarkGray)
            .then(&"\u{25a0}".repeat(filled)).color(NamedColor::Aqua)
            .then(&"\u{25a0}".repeat(25 - filled)).color(NamedColor::Gray)
            .then("]").color(NamedColor::DarkGray)
            .build()
    }
}


impl Shape for DivisionSection {
    fn draw(&self, ctx: &mut DrawContext) {
        let section_height = 53.0;
        let bottom_padding = 13.0;
        let font_size = LEVEL_SCALE * 16.0;
        let available_width = CANVAS_WIDTH - 2 * LEVEL_PADDING;

        let current = self.current_text();
        let right = self.right_text();
        let progress_bar = self.progress_bar_text();

        let (current_w, star_h) = ctx.renderer.measure(&current, font_size);
        let (right_w, _) = ctx.renderer.measure(&right, font_size);
        let spacing = font_size * 0.3;
        let bar_available = available_width as f32 - current_w - right_w - spacing * 2.0;

        let (bar_w, bar_h) = ctx.renderer.measure(&progress_bar, font_size);
        let (bar_scale, scaled_bar_w, bar_h) = if bar_w > bar_available {
            let s = LEVEL_SCALE * (bar_available / bar_w);
            let (w, h) = ctx.renderer.measure(&progress_bar, s * 16.0);
            (s, w, h)
        } else {
            (LEVEL_SCALE, bar_w, bar_h)
        };

        let total_w = current_w + spacing + scaled_bar_w + spacing + right_w;
        let start_x = LEVEL_PADDING as f32 + (available_width as f32 - total_w) / 2.0;
        let star_y = section_height - star_h - bottom_padding;
        let star_center_y = star_y + star_h / 2.0;
        let bar_y = (star_center_y - bar_h / 2.0) as i32;
        let star_y = star_y as i32;
        let (cw, ch) = ctx.buffer.dimensions();

        ctx.renderer.draw(
            ctx.buffer.as_mut(),
            cw,
            ch,
            ctx.x as f32 + start_x,
            (ctx.y + star_y) as f32,
            &current,
            font_size,
            true,
        );
        let bar_x = start_x + current_w + spacing;
        ctx.renderer.draw(
            ctx.buffer.as_mut(),
            cw,
            ch,
            ctx.x as f32 + bar_x,
            ctx.y as f32 + bar_y as f32,
            &progress_bar,
            bar_scale * 16.0,
            true,
        );
        let next_x = bar_x + scaled_bar_w + spacing;
        ctx.renderer.draw(
            ctx.buffer.as_mut(),
            cw,
            ch,
            ctx.x as f32 + next_x,
            (ctx.y + star_y) as f32,
            &right,
            font_size,
            true,
        );
    }

    fn size(&self) -> (u32, u32) {
        (CANVAS_WIDTH, 53)
    }
}


struct SkinSection<'a> {
    skin: Option<&'a DynamicImage>,
    network_level: f64,
    subtitle: &'a str,
}


impl<'a> SkinSection<'a> {
    fn new(skin: Option<&'a DynamicImage>, network_level: f64, subtitle: &'a str) -> Self {
        Self {
            skin,
            network_level,
            subtitle,
        }
    }
}


impl Shape for SkinSection<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        RoundedRect::new(COL_WIDTH, SKIN_BOX_HEIGHT)
            .corner_radius(BOX_CORNER_RADIUS)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        let level_scale = 2.0;
        let mode_scale = 1.5;
        let level_text_height = (level_scale * 16.0) as u32;
        let mode_text_height = (mode_scale * 16.0) as u32;

        let level_text = MCText::new()
            .span("Level ")
            .color(NamedColor::Gray)
            .then(&{
                let s = format!("{:.2}", self.network_level);
                s.strip_suffix(".00").map(String::from).unwrap_or(s)
            })
            .color(NamedColor::Yellow)
            .build();
        TextBlock::new().push(level_text).scale(level_scale).align_x(Align::Center).max_width(COL_WIDTH).draw(&mut ctx.at(0, SKIN_PADDING as i32));

        let mode_y = SKIN_BOX_HEIGHT - SKIN_PADDING - mode_text_height;
        TextBlock::new().push(MCText::new().span(&format!("({})", self.subtitle)).color(NamedColor::Gray).build()).scale(mode_scale).align_x(Align::Center).max_width(COL_WIDTH).draw(&mut ctx.at(0, mode_y as i32));

        if let Some(skin) = &self.skin {
            let level_bottom = SKIN_PADDING + level_text_height;
            let available_h = mode_y - level_bottom;
            let max_w = COL_WIDTH - 26;
            let (orig_w, orig_h) = (skin.width(), skin.height());
            let scale = f64::min(max_w as f64 / orig_w as f64, available_h as f64 / orig_h as f64);
            let new_w = (orig_w as f64 * scale) as u32;
            let new_h = (orig_h as f64 * scale) as u32;
            let skin_x = (COL_WIDTH - new_w) / 2;
            let skin_y = level_bottom + (available_h - new_h) / 2 + 12;
            Image::new(skin).size(new_w, new_h).draw(&mut ctx.at(skin_x as i32, skin_y as i32));
        }
    }

    fn size(&self) -> (u32, u32) {
        (COL_WIDTH, SKIN_BOX_HEIGHT)
    }
}


struct StatsSection<'a> {
    stats: &'a DuelsViewStats,
}


impl<'a> StatsSection<'a> {
    fn new(stats: &'a DuelsViewStats, _is_delta: bool) -> Self {
        Self { stats }
    }
}


impl Shape for StatsSection<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        RoundedRect::new(STATS_BOX_WIDTH, STATS_BOX_HEIGHT)
            .corner_radius(BOX_CORNER_RADIUS)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        let main_scale = 2.0;
        let aux_scale = 1.5;
        let main_font = main_scale * 16.0;
        let aux_font = aux_scale * 16.0;
        let padding = 16;
        let line_height = (STATS_BOX_HEIGHT - padding * 2) / 4;

        let wlr = ratio(self.stats.wins, self.stats.losses);
        let kdr = ratio(self.stats.kills, self.stats.deaths);

        let left_entries: [(MCText, Option<MCText>); 4] = [
            (stat_line("Wins: ", &format_number(self.stats.wins), colors::wins(self.stats.wins)), None),
            (stat_line("Kills: ", &format_number(self.stats.kills), colors::kills(self.stats.kills)), None),
            (stat_line("Losses: ", &format_number(self.stats.losses), NamedColor::Red), None),
            (stat_line("Deaths: ", &format_number(self.stats.deaths), NamedColor::Red), None),
        ];

        let right_entries: [MCText; 4] = [
            stat_line("W/L: ", &format_ratio(wlr), colors::wlr(wlr)),
            stat_line("K/D: ", &format_ratio(kdr), colors::kdr(kdr)),
            stat_line("Melee %: ", &percent(self.stats.melee_hits, self.stats.melee_swings), NamedColor::Aqua),
            stat_line("Arrow %: ", &percent(self.stats.bow_hits, self.stats.bow_shots), NamedColor::Aqua),
        ];

        let mut max_left_w: f32 = 0.0;
        let mut left_measured = Vec::new();
        for (main_text, aux_text) in &left_entries {
            let (main_w, main_h) = ctx.renderer.measure(main_text, main_font);
            let (aux_w, aux_h) = aux_text
                .as_ref()
                .map(|t| ctx.renderer.measure(t, aux_font))
                .unwrap_or((0.0, 0.0));
            max_left_w = max_left_w.max(main_w + aux_w);
            left_measured.push((main_w, main_h, aux_w, aux_h));
        }

        let mut max_right_w: f32 = 0.0;
        let mut right_measured = Vec::new();
        for entry in &right_entries {
            let (w, h) = ctx.renderer.measure(entry, main_font);
            max_right_w = max_right_w.max(w);
            right_measured.push((w, h));
        }

        let right_x = (STATS_BOX_WIDTH - COL_WIDTH + padding) as f32;

        for (i, ((main_text, aux_text), (main_w, main_h, _aux_w, aux_h))) in
            left_entries.into_iter().zip(left_measured.iter()).enumerate()
        {
            let y = padding + i as u32 * line_height;
            TextBlock::new().push(main_text).scale(main_scale).draw(&mut ctx.at(padding as i32, y as i32));
            if let Some(aux) = aux_text {
                let aux_y = y as f32 + (main_h - aux_h) * 0.75;
                TextBlock::new().push(aux).scale(aux_scale).draw(&mut ctx.at((padding as f32 + main_w) as i32, aux_y as i32));
            }
        }

        for (i, (entry, (_w, _h))) in right_entries.into_iter().zip(right_measured.iter()).enumerate() {
            let y = padding + i as u32 * line_height;
            TextBlock::new().push(entry).scale(main_scale).draw(&mut ctx.at(right_x as i32, y as i32));
        }
    }

    fn size(&self) -> (u32, u32) {
        (STATS_BOX_WIDTH, STATS_BOX_HEIGHT)
    }
}


struct BreakdownBox<'a> {
    entries: &'a [DuelsBreakdownEntry],
    is_overall: bool,
    overall_stats: Option<&'a DuelsViewStats>,
    view_stats: Option<&'a DuelsViewStats>,
}


impl<'a> BreakdownBox<'a> {
    fn overall(entries: &'a [DuelsBreakdownEntry]) -> Self {
        Self { entries, is_overall: true, overall_stats: None, view_stats: None }
    }

    fn mode_share(entries: &'a [DuelsBreakdownEntry], overall: &'a DuelsViewStats, view: &'a DuelsViewStats) -> Self {
        Self { entries, is_overall: false, overall_stats: Some(overall), view_stats: Some(view) }
    }
}


impl Shape for BreakdownBox<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        RoundedRect::new(COL_WIDTH, SECOND_ROW_HEIGHT)
            .corner_radius(BOX_CORNER_RADIUS)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        if !self.is_overall {
            if let (Some(overall), Some(view)) = (self.overall_stats, self.view_stats) {
                self.draw_mode_share(ctx, overall, view);
                return;
            }
        }

        self.draw_top_played(ctx);
    }

    fn size(&self) -> (u32, u32) {
        (COL_WIDTH, SECOND_ROW_HEIGHT)
    }
}


impl BreakdownBox<'_> {
    fn draw_top_played(&self, ctx: &mut DrawContext) {
        let padding = 16u32;
        let bar_height = 28u32;
        let text_scale = 1.5f32;
        let text_font = text_scale * 16.0;

        let total_games: u64 = self.entries.iter().map(|e| e.wins + e.losses).sum();
        let bar_width = COL_WIDTH - padding * 2;
        let gap = (SECOND_ROW_HEIGHT - padding * 2 - bar_height * 4) / 3;

        for (i, entry) in self.entries.iter().take(4).enumerate() {
            let games = entry.wins + entry.losses;
            let pct = if total_games == 0 { 0.0 } else { games as f64 / total_games as f64 * 100.0 };
            let bx = padding;
            let by = padding + i as u32 * (bar_height + gap);
            let filled_w = (pct / 100.0 * bar_width as f64).round() as u32;
            if filled_w > 0 {
                draw_progress_bar(ctx, bx, by, filled_w, bar_height, 0, 1.0, BAR_COLOR, BAR_COLOR);
            }
            let text = MCText::new()
                .span(&format_percent(pct)).color(NamedColor::Green)
                .then(&format!(" {}", entry.label)).color(NamedColor::Gray)
                .build();
            let (cw, ch) = ctx.buffer.dimensions();
            let (tw, th) = ctx.renderer.measure(&text, text_font);
            ctx.renderer.draw(
                ctx.buffer.as_mut(), cw, ch,
                ctx.x as f32 + bx as f32 + (bar_width as f32 - tw) / 2.0,
                ctx.y as f32 + by as f32 + (bar_height as f32 - th) / 2.0,
                &text, text_font, true,
            );
        }
    }

    fn draw_mode_share(&self, ctx: &mut DrawContext, overall: &DuelsViewStats, view: &DuelsViewStats) {
        let padding = 16u32;
        let bar_height = 28u32;
        let text_scale = 1.5f32;
        let text_font = text_scale * 16.0;

        let pct = |a: u64, b: u64| if b == 0 { 0.0 } else { a as f64 / b as f64 * 100.0 };

        let rows: [(&str, f64); 4] = [
            ("Wins", pct(view.wins, overall.wins)),
            ("Kills", pct(view.kills, overall.kills)),
            ("Losses", pct(view.losses, overall.losses)),
            ("Deaths", pct(view.deaths, overall.deaths)),
        ];

        let bar_width = COL_WIDTH - padding * 2;
        let gap = (SECOND_ROW_HEIGHT - padding * 2 - bar_height * 4) / 3;
        let (cw, ch) = ctx.buffer.dimensions();

        for (i, (label, pct_val)) in rows.iter().enumerate() {
            let bx = padding;
            let by = padding + i as u32 * (bar_height + gap);
            let filled_w = (pct_val / 100.0 * bar_width as f64).round() as u32;
            if filled_w > 0 {
                draw_progress_bar(ctx, bx, by, filled_w, bar_height, 0, 1.0, BAR_COLOR, BAR_COLOR);
            }
            let text = MCText::new()
                .span(&format_percent(*pct_val)).color(NamedColor::Green)
                .then(&format!(" of {label}")).color(NamedColor::Gray)
                .build();
            let (tw, th) = ctx.renderer.measure(&text, text_font);
            ctx.renderer.draw(
                ctx.buffer.as_mut(), cw, ch,
                ctx.x as f32 + bx as f32 + (bar_width as f32 - tw) / 2.0,
                ctx.y as f32 + by as f32 + (bar_height as f32 - th) / 2.0,
                &text, text_font, true,
            );
        }
    }
}


struct DuelsWinstreaksBox<'a> {
    winstreaks: &'a WinstreakHistory,
    current_ws: Option<u64>,
}


impl Shape for DuelsWinstreaksBox<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        let padding = 12u32;
        let scale = 1.5f32;
        let font = scale * 16.0;
        let inner_w = COL_WIDTH - padding * 2;

        RoundedRect::new(COL_WIDTH, SECOND_ROW_HEIGHT)
            .corner_radius(BOX_CORNER_RADIUS)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        let current_line = match self.current_ws {
            Some(ws) => MCText::new().span("Winstreak: ").color(NamedColor::Gray).then(&format_number(ws)).color(colors::winstreak(ws)).build(),
            None => MCText::new().span("Winstreak: ").color(NamedColor::Gray).then("?").color(NamedColor::Red).build(),
        };

        let (_, line_h) = ctx.renderer.measure(&current_line, font);
        let mut y = padding as f32;
        TextBlock::new().push(current_line).scale(scale).draw(&mut ctx.at(padding as i32, y as i32));
        y += line_h;

        let display_count = self.winstreaks.streaks.len().min(MAX_DISPLAYED_STREAKS);
        let icon_size = 20u32;
        let icon_radius = 8u32;
        let icon_gap = 4u32;
        let urchin_icon = crate::icons::urchin(icon_size, icon_radius);
        let antisniper_icon = crate::icons::antisniper(icon_size, icon_radius);

        for (i, streak) in self.winstreaks.streaks[..display_count].iter().enumerate() {
            let suffix = if streak.approximate { "+" } else { "" };
            let date = format_timestamp(streak.timestamp.timestamp_millis());
            let color = colors::winstreak(streak.value);
            let rank = format!("{}.", i + 1);

            let icon = match streak.source {
                StreakSource::Urchin => &urchin_icon,
                StreakSource::Antisniper => &antisniper_icon,
            };
            Image::new(icon).draw(&mut ctx.at(padding as i32, (y + (line_h - icon_size as f32) / 2.0) as i32));

            let text_x = padding + icon_size + icon_gap;
            let left = MCText::new()
                .span(&rank)
                .color(NamedColor::DarkGray)
                .then(" ")
                .then(&format!("{}{}", format_number(streak.value), suffix))
                .color(color)
                .build();
            let right = MCText::new()
                .span("- ")
                .color(NamedColor::DarkGray)
                .then(&date)
                .color(NamedColor::Gray)
                .build();

            TextBlock::new().push(left).scale(scale).draw(&mut ctx.at(text_x as i32, y as i32));
            let (rw, _) = ctx.renderer.measure(&right, font);
            TextBlock::new().push(right).scale(scale).draw(&mut ctx.at((padding as f32 + inner_w as f32 - rw) as i32, y as i32));
            y += line_h;
        }
    }

    fn size(&self) -> (u32, u32) {
        (COL_WIDTH, SECOND_ROW_HEIGHT)
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
        .width(COL_WIDTH)
        .height(BOTTOM_BOX_HEIGHT)
        .corner_radius(BOX_CORNER_RADIUS)
        .padding(12, 12)
        .scale(1.5)
        .line_spacing(0.0)
        .align_x(Align::Center)
        .align_y(Align::Spread)
        .push(
            MCText::new()
                .span("Session: ")
                .color(NamedColor::Gray)
                .then(session_type.display_name())
                .color(NamedColor::White)
                .build(),
        )
        .push(
            MCText::new()
                .span("Start: ")
                .color(NamedColor::Gray)
                .then(&start_str)
                .color(NamedColor::White)
                .build(),
        )
        .push(
            MCText::new()
                .span("Duration: ")
                .color(NamedColor::Gray)
                .then(&duration_str)
                .color(NamedColor::Green)
                .build(),
        )
}


fn status_box(first_login: Option<i64>) -> TextBox {
    let status = MCText::new()
        .span("Status: ")
        .color(NamedColor::Gray)
        .then("N/A")
        .color(NamedColor::Gray)
        .build();
    let last_login = MCText::new()
        .span("Last Login: ")
        .color(NamedColor::Gray)
        .then("N/A")
        .color(NamedColor::Gray)
        .build();
    let first_login = first_login
        .map(|ts| {
            MCText::new()
                .span("First Login: ")
                .color(NamedColor::Gray)
                .then(&format_timestamp(ts))
                .color(NamedColor::White)
                .build()
        })
        .unwrap_or_else(|| {
            MCText::new()
                .span("First Login: ")
                .color(NamedColor::Gray)
                .then("N/A")
                .color(NamedColor::Gray)
                .build()
        });

    TextBox::new()
        .width(COL_WIDTH)
        .height(BOTTOM_BOX_HEIGHT)
        .corner_radius(BOX_CORNER_RADIUS)
        .padding(12, 12)
        .scale(1.5)
        .line_spacing(0.0)
        .align_x(Align::Center)
        .align_y(Align::Spread)
        .push(status)
        .push(last_login)
        .push(first_login)
}


fn view_box(stats: &DuelsViewStats) -> TextBox {
    let div = stats.division.0;
    let div_color = division_color(div.color_name);
    let div_name = format_division_name(stats.division);
    let (current_wins, next_wins) = win_progress(stats.wins, stats.track);

    let mut div_span = MCText::new().span("Title: ").color(NamedColor::Gray).then(&div_name).color(div_color);
    if div.bold { div_span = div_span.bold(); }

    TextBox::new()
        .width(COL_WIDTH)
        .height(BOTTOM_BOX_HEIGHT)
        .corner_radius(BOX_CORNER_RADIUS)
        .padding(12, 12)
        .scale(1.5)
        .line_spacing(0.0)
        .align_x(Align::Center)
        .align_y(Align::Spread)
        .push(div_span.build())
        .push(
            MCText::new()
                .span("Progress: ").color(NamedColor::Gray)
                .then(&format_compact(current_wins)).color(NamedColor::Aqua)
                .then("/").color(NamedColor::Gray)
                .then(&format_compact(next_wins)).color(NamedColor::Green)
                .build(),
        )
        .push(
            MCText::new()
                .span("Goals: ").color(NamedColor::Gray)
                .then(&if stats.show_goals { format_number(stats.goals) } else { "-".to_string() })
                .color(NamedColor::Green)
                .build(),
        )
}


struct DuelsGuildBox<'a> {
    guild: &'a GuildInfo,
}


impl<'a> DuelsGuildBox<'a> {
    fn new(guild: &'a GuildInfo) -> Self {
        Self { guild }
    }
}


impl Shape for DuelsGuildBox<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        RoundedRect::new(COL_WIDTH, BOTTOM_BOX_HEIGHT)
            .corner_radius(BOX_CORNER_RADIUS)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        let scale = 1.5;
        let font = scale * 16.0;
        let padding = 12u32;
        let inner_w = COL_WIDTH - padding * 2;

        let name = self.guild.name.as_deref().unwrap_or("-");
        let rank = self.guild.rank.as_deref().unwrap_or("N/A");
        let joined = self
            .guild
            .joined
            .map(format_timestamp)
            .unwrap_or_else(|| "N/A".to_string());
        let color = self
            .guild
            .tag_color
            .as_ref()
            .and_then(|c| color_name_to_named(c))
            .unwrap_or(NamedColor::Gray);

        let lines = [
            MCText::new().span(name).color(color).build(),
            MCText::new()
                .span("Rank: ")
                .color(NamedColor::Gray)
                .then(rank)
                .color(color)
                .build(),
            MCText::new()
                .span("Joined: ")
                .color(NamedColor::Gray)
                .then(&joined)
                .color(NamedColor::White)
                .build(),
        ];

        let measurements: Vec<(f32, f32)> = lines
            .iter()
            .map(|l| ctx.renderer.measure(l, font))
            .collect();
        let total_h: f32 = measurements.iter().map(|(_, h)| h).sum();
        let spacing = (BOTTOM_BOX_HEIGHT as f32 - padding as f32 * 2.0 - total_h)
            / (lines.len() - 1).max(1) as f32;

        let mut y = padding as f32;
        for (line_text, (tw, lh)) in lines.into_iter().zip(measurements) {
            let effective_h = if tw > inner_w as f32 {
                ctx.renderer
                    .measure(&line_text, scale * (inner_w as f32 / tw) * 16.0)
                    .1
            } else {
                lh
            };
            let y_offset = (lh - effective_h) / 2.0;
            TextBlock::new()
                .push(line_text)
                .scale(scale)
                .max_width(inner_w)
                .align_x(Align::Center)
                .draw(&mut ctx.at(padding as i32, (y + y_offset) as i32));
            y += lh + spacing;
        }
    }

    fn size(&self) -> (u32, u32) {
        (COL_WIDTH, BOTTOM_BOX_HEIGHT)
    }
}


fn format_compact(n: u64) -> String {
    if n >= 10_000 {
        let k = n as f64 / 1000.0;
        let s = format!("{:.1}k", k);
        s.strip_suffix(".0k").map(|v| format!("{v}k")).unwrap_or(s)
    } else {
        format_number(n)
    }
}


fn ratio(a: u64, b: u64) -> f64 {
    if b == 0 {
        a as f64
    } else {
        a as f64 / b as f64
    }
}


fn percent(hits: u64, attempts: u64) -> String {
    if attempts == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", hits as f64 / attempts as f64 * 100.0)
    }
}


fn format_division_name((division, level): (DuelsDivision, u32)) -> String {
    if division.name == "None" || level == 0 {
        "-".to_string()
    } else if level == 1 {
        division.name.to_string()
    } else {
        format!("{} {}", division.name, roman(level))
    }
}


fn division_color(name: &str) -> NamedColor {
    color_name_to_named(name).unwrap_or(NamedColor::Gray)
}


fn roman(value: u32) -> String {
    let numerals = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut remaining = value;
    let mut out = String::new();
    for (number, symbol) in numerals {
        while remaining >= number {
            out.push_str(symbol);
            remaining -= number;
        }
    }
    out
}
