use chrono::{DateTime, Utc};
use image::Rgba;
use mctext::{MCText, NamedColor};

use crate::canvas::{
    Align, BOX_BACKGROUND, DrawContext, RoundedRect, Shape, TextBlock, blend,
};


pub type TagIcon = (String, u32);


#[derive(Clone)]
pub enum SessionType {
    Custom(String),
    Daily,
    Weekly,
    Monthly,
    Yearly,
}


impl SessionType {
    pub fn display_name(&self) -> &str {
        match self {
            SessionType::Custom(name) => name,
            SessionType::Daily => "Daily",
            SessionType::Weekly => "Weekly",
            SessionType::Monthly => "Monthly",
            SessionType::Yearly => "Yearly",
        }
    }
}


pub struct ModeGames {
    pub solos: u64,
    pub doubles: u64,
    pub threes: u64,
    pub fours: u64,
    pub four_v_four: u64,
}


struct ModeEntry {
    label: &'static str,
    count: u64,
    color: NamedColor,
}


impl ModeGames {
    fn total(&self) -> u64 {
        self.solos + self.doubles + self.threes + self.fours + self.four_v_four
    }

    fn entries(&self) -> Vec<ModeEntry> {
        [("1s", self.solos), ("2s", self.doubles), ("3s", self.threes), ("4s", self.fours), ("4v4", self.four_v_four)]
            .into_iter()
            .map(|(label, count)| ModeEntry { label, count, color: NamedColor::Green })
            .collect()
    }
}


pub struct VerticalGamesBox<'a> {
    mode_games: &'a ModeGames,
    width: u32,
    height: u32,
}


impl<'a> VerticalGamesBox<'a> {
    pub fn new(mode_games: &'a ModeGames, width: u32, height: u32) -> Self {
        Self { mode_games, width, height }
    }
}


impl Shape for VerticalGamesBox<'_> {
    fn draw(&self, ctx: &mut DrawContext) {
        let padding = 12u32;
        let scale = 1.5f32;
        let font = scale * 16.0;
        let label_scale = 1.25f32;
        let label_font = label_scale * 16.0;

        RoundedRect::new(self.width, self.height)
            .corner_radius(18)
            .background(BOX_BACKGROUND)
            .draw(ctx);

        let total = self.mode_games.total();
        let entries = self.mode_games.entries();

        let title = MCText::new()
            .span("Games: ").color(NamedColor::Gray)
            .then(&format_number(total)).color(NamedColor::White)
            .build();
        let (_, title_h) = ctx.renderer.measure(&title, font);
        TextBlock::new()
            .push(title).scale(scale).align_x(Align::Center).max_width(self.width)
            .draw(&mut ctx.at(0, padding as i32));

        if entries.is_empty() { return; }

        let sample_label = MCText::new().span("4v4").color(NamedColor::Gray).build();
        let (_, label_h) = ctx.renderer.measure(&sample_label, label_font);

        let bar_top = padding + title_h as u32 + 8;
        let bar_bottom = self.height - padding - label_h as u32 - 4;
        let max_bar_h = bar_bottom.saturating_sub(bar_top);
        let inner_w = self.width - padding * 2;
        let max_count = entries.iter().map(|e| e.count).max().unwrap_or(1).max(1);

        let bar_count = entries.len() as u32;
        let gap = 6u32;
        let bar_w = inner_w.saturating_sub(gap * bar_count.saturating_sub(1)) / bar_count;
        let origin = (ctx.x, ctx.y);
        let (bw, bh) = ctx.buffer.dimensions();

        for (i, entry) in entries.iter().enumerate() {
            let x = padding + i as u32 * (bar_w + gap);
            let bar_h = (entry.count as f64 / max_count as f64 * max_bar_h as f64).round() as u32;
            let bar_h = bar_h.max(2);
            let bar_y = bar_bottom - bar_h;

            let bg_color = Rgba([50, 50, 55, 220]);
            for py in bar_y..bar_bottom {
                for px in x..x + bar_w {
                    let abs_x = (origin.0 + px as i32) as u32;
                    let abs_y = (origin.1 + py as i32) as u32;
                    if abs_x < bw && abs_y < bh {
                        let bg = *ctx.buffer.get_pixel(abs_x, abs_y);
                        ctx.buffer.put_pixel(abs_x, abs_y, blend(bg, bg_color));
                    }
                }
            }

            if total > 0 && entry.count > 0 {
                let pct = (entry.count as f64 / total as f64 * 100.0).round() as u32;
                let pct_text = MCText::new().span(&format!("{}%", pct)).color(NamedColor::Green).build();
                let pct_font = 1.1f32 * 16.0;
                let (pw, ph) = ctx.renderer.measure(&pct_text, pct_font);
                if (ph as u32) + 4 <= bar_h && (pw as u32) + 2 <= bar_w {
                    ctx.renderer.draw(
                        ctx.buffer.as_mut(), bw, bh,
                        origin.0 as f32 + x as f32 + (bar_w as f32 - pw) / 2.0,
                        (origin.1 + bar_y as i32 + ((bar_h as f32 - ph) / 2.0) as i32) as f32,
                        &pct_text, pct_font, true,
                    );
                }
            }

            let label = MCText::new().span(entry.label).color(entry.color).build();
            let (lw, _) = ctx.renderer.measure(&label, label_font);
            ctx.renderer.draw(
                ctx.buffer.as_mut(), bw, bh,
                origin.0 as f32 + x as f32 + (bar_w as f32 - lw) / 2.0,
                (origin.1 + (bar_bottom + 4) as i32) as f32,
                &label, label_font, true,
            );
        }
    }

    fn size(&self) -> (u32, u32) { (self.width, self.height) }
}


pub mod bedwars_colors {
    use mctext::NamedColor;

    pub fn wlr(v: f64) -> NamedColor {
        match v {
            v if v >= 30.0 => NamedColor::DarkPurple,
            v if v >= 15.0 => NamedColor::LightPurple,
            v if v >= 9.0 => NamedColor::DarkRed,
            v if v >= 6.0 => NamedColor::Red,
            v if v >= 3.0 => NamedColor::Gold,
            v if v >= 2.1 => NamedColor::Yellow,
            v if v >= 1.5 => NamedColor::DarkGreen,
            v if v >= 0.9 => NamedColor::Green,
            v if v >= 0.3 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn fkdr(v: f64) -> NamedColor {
        match v {
            v if v >= 100.0 => NamedColor::DarkPurple,
            v if v >= 50.0 => NamedColor::LightPurple,
            v if v >= 30.0 => NamedColor::DarkRed,
            v if v >= 20.0 => NamedColor::Red,
            v if v >= 10.0 => NamedColor::Gold,
            v if v >= 7.0 => NamedColor::Yellow,
            v if v >= 5.0 => NamedColor::DarkGreen,
            v if v >= 3.0 => NamedColor::Green,
            v if v >= 1.0 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn kdr(v: f64) -> NamedColor {
        match v {
            v if v >= 8.0 => NamedColor::DarkPurple,
            v if v >= 7.0 => NamedColor::LightPurple,
            v if v >= 6.0 => NamedColor::DarkRed,
            v if v >= 5.0 => NamedColor::Red,
            v if v >= 4.0 => NamedColor::Gold,
            v if v >= 3.0 => NamedColor::Yellow,
            v if v >= 2.0 => NamedColor::DarkGreen,
            v if v >= 1.0 => NamedColor::Green,
            v if v >= 0.5 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn bblr(v: f64) -> NamedColor {
        match v {
            v if v >= 20.0 => NamedColor::DarkPurple,
            v if v >= 10.0 => NamedColor::LightPurple,
            v if v >= 6.0 => NamedColor::DarkRed,
            v if v >= 4.0 => NamedColor::Red,
            v if v >= 2.0 => NamedColor::Gold,
            v if v >= 1.4 => NamedColor::Yellow,
            v if v >= 1.0 => NamedColor::DarkGreen,
            v if v >= 0.6 => NamedColor::Green,
            v if v >= 0.2 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn session_wlr(v: f64) -> NamedColor {
        match v {
            v if v >= 150.0 => NamedColor::DarkPurple,
            v if v >= 75.0 => NamedColor::LightPurple,
            v if v >= 45.0 => NamedColor::DarkRed,
            v if v >= 30.0 => NamedColor::Red,
            v if v >= 15.0 => NamedColor::Gold,
            v if v >= 10.5 => NamedColor::Yellow,
            v if v >= 7.5 => NamedColor::DarkGreen,
            v if v >= 4.5 => NamedColor::Green,
            v if v >= 1.5 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn session_fkdr(v: f64) -> NamedColor {
        match v {
            v if v >= 500.0 => NamedColor::DarkPurple,
            v if v >= 250.0 => NamedColor::LightPurple,
            v if v >= 150.0 => NamedColor::DarkRed,
            v if v >= 100.0 => NamedColor::Red,
            v if v >= 50.0 => NamedColor::Gold,
            v if v >= 35.0 => NamedColor::Yellow,
            v if v >= 25.0 => NamedColor::DarkGreen,
            v if v >= 15.0 => NamedColor::Green,
            v if v >= 5.0 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn session_bblr(v: f64) -> NamedColor {
        match v {
            v if v >= 100.0 => NamedColor::DarkPurple,
            v if v >= 50.0 => NamedColor::LightPurple,
            v if v >= 30.0 => NamedColor::DarkRed,
            v if v >= 20.0 => NamedColor::Red,
            v if v >= 10.0 => NamedColor::Gold,
            v if v >= 7.0 => NamedColor::Yellow,
            v if v >= 5.0 => NamedColor::DarkGreen,
            v if v >= 3.0 => NamedColor::Green,
            v if v >= 1.0 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn wins(v: u64) -> NamedColor {
        match v {
            v if v >= 30000 => NamedColor::DarkPurple,
            v if v >= 15000 => NamedColor::LightPurple,
            v if v >= 7500 => NamedColor::DarkRed,
            v if v >= 4500 => NamedColor::Red,
            v if v >= 2250 => NamedColor::Gold,
            v if v >= 1500 => NamedColor::Yellow,
            v if v >= 450 => NamedColor::DarkGreen,
            v if v >= 300 => NamedColor::Green,
            v if v >= 150 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn final_kills(v: u64) -> NamedColor {
        match v {
            v if v >= 100000 => NamedColor::DarkPurple,
            v if v >= 50000 => NamedColor::LightPurple,
            v if v >= 25000 => NamedColor::DarkRed,
            v if v >= 15000 => NamedColor::Red,
            v if v >= 7500 => NamedColor::Gold,
            v if v >= 5000 => NamedColor::Yellow,
            v if v >= 2500 => NamedColor::DarkGreen,
            v if v >= 1000 => NamedColor::Green,
            v if v >= 500 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn kills(v: u64) -> NamedColor {
        match v {
            v if v >= 75000 => NamedColor::DarkPurple,
            v if v >= 37500 => NamedColor::LightPurple,
            v if v >= 18750 => NamedColor::DarkRed,
            v if v >= 11250 => NamedColor::Red,
            v if v >= 5625 => NamedColor::Gold,
            v if v >= 3750 => NamedColor::Yellow,
            v if v >= 1875 => NamedColor::DarkGreen,
            v if v >= 750 => NamedColor::Green,
            v if v >= 375 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn beds_broken(v: u64) -> NamedColor {
        match v {
            v if v >= 50000 => NamedColor::DarkPurple,
            v if v >= 25000 => NamedColor::LightPurple,
            v if v >= 12500 => NamedColor::DarkRed,
            v if v >= 7500 => NamedColor::Red,
            v if v >= 3750 => NamedColor::Gold,
            v if v >= 2500 => NamedColor::Yellow,
            v if v >= 1250 => NamedColor::DarkGreen,
            v if v >= 500 => NamedColor::Green,
            v if v >= 250 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn winstreak(v: u64) -> NamedColor {
        match v {
            v if v >= 500 => NamedColor::DarkPurple,
            v if v >= 250 => NamedColor::LightPurple,
            v if v >= 100 => NamedColor::DarkRed,
            v if v >= 75 => NamedColor::Red,
            v if v >= 50 => NamedColor::Gold,
            v if v >= 40 => NamedColor::Yellow,
            v if v >= 25 => NamedColor::DarkGreen,
            v if v >= 15 => NamedColor::Green,
            v if v >= 5 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }
}


pub mod duels_colors {
    use mctext::NamedColor;

    pub fn wlr(v: f64) -> NamedColor {
        match v {
            v if v >= 42.0 => NamedColor::DarkPurple,
            v if v >= 20.0 => NamedColor::LightPurple,
            v if v >= 12.5 => NamedColor::DarkRed,
            v if v >= 8.2 => NamedColor::Red,
            v if v >= 4.4 => NamedColor::Gold,
            v if v >= 3.2 => NamedColor::Yellow,
            v if v >= 1.9 => NamedColor::DarkGreen,
            v if v >= 1.2 => NamedColor::Green,
            v if v >= 0.4 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn kdr(v: f64) -> NamedColor {
        match v {
            v if v >= 33.0 => NamedColor::DarkPurple,
            v if v >= 16.0 => NamedColor::LightPurple,
            v if v >= 10.0 => NamedColor::DarkRed,
            v if v >= 6.5 => NamedColor::Red,
            v if v >= 3.5 => NamedColor::Gold,
            v if v >= 2.5 => NamedColor::Yellow,
            v if v >= 1.5 => NamedColor::DarkGreen,
            v if v >= 1.0 => NamedColor::Green,
            v if v >= 0.3 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn wins(v: u64) -> NamedColor {
        match v {
            v if v >= 95000 => NamedColor::DarkPurple,
            v if v >= 48000 => NamedColor::LightPurple,
            v if v >= 24000 => NamedColor::DarkRed,
            v if v >= 14000 => NamedColor::Red,
            v if v >= 7000 => NamedColor::Gold,
            v if v >= 4750 => NamedColor::Yellow,
            v if v >= 2250 => NamedColor::DarkGreen,
            v if v >= 1000 => NamedColor::Green,
            v if v >= 450 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn kills(v: u64) -> NamedColor {
        match v {
            v if v >= 75000 => NamedColor::DarkPurple,
            v if v >= 38000 => NamedColor::LightPurple,
            v if v >= 19000 => NamedColor::DarkRed,
            v if v >= 11000 => NamedColor::Red,
            v if v >= 5500 => NamedColor::Gold,
            v if v >= 3750 => NamedColor::Yellow,
            v if v >= 1750 => NamedColor::DarkGreen,
            v if v >= 750 => NamedColor::Green,
            v if v >= 350 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }

    pub fn winstreak(v: u64) -> NamedColor {
        match v {
            v if v >= 1000 => NamedColor::DarkPurple,
            v if v >= 500 => NamedColor::LightPurple,
            v if v >= 200 => NamedColor::DarkRed,
            v if v >= 150 => NamedColor::Red,
            v if v >= 100 => NamedColor::Gold,
            v if v >= 80 => NamedColor::Yellow,
            v if v >= 50 => NamedColor::DarkGreen,
            v if v >= 30 => NamedColor::Green,
            v if v >= 10 => NamedColor::White,
            _ => NamedColor::Gray,
        }
    }
}


pub fn format_ratio(value: f64) -> String {
    let s = format!("{:.2}", value);
    s.strip_suffix(".00").map(String::from).unwrap_or(s)
}


pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.insert(0, ','); }
        result.insert(0, c);
    }
    result
}


pub fn format_percent(value: f64) -> String {
    let s = format!("{:.1}", value);
    format!("{}%", s.strip_suffix(".0").unwrap_or(&s))
}


pub fn format_timestamp(ts: i64) -> String {
    let ts_millis = if ts > 10_000_000_000 { ts } else { ts * 1000 };
    DateTime::<Utc>::from_timestamp_millis(ts_millis)
        .map(|dt| dt.format("%m/%d/%y").to_string())
        .unwrap_or_else(|| "N/A".to_string())
}


pub fn stat_line(label: &str, value: &str, color: NamedColor) -> MCText {
    MCText::new().span(label).color(NamedColor::Gray).then(value).color(color).build()
}


pub fn color_name_to_named(name: &str) -> Option<NamedColor> {
    match name.to_uppercase().as_str() {
        "DARK_GREEN" => Some(NamedColor::DarkGreen),
        "DARK_AQUA" => Some(NamedColor::DarkAqua),
        "DARK_RED" => Some(NamedColor::DarkRed),
        "DARK_PURPLE" => Some(NamedColor::DarkPurple),
        "GOLD" => Some(NamedColor::Gold),
        "GRAY" => Some(NamedColor::Gray),
        "DARK_GRAY" => Some(NamedColor::DarkGray),
        "BLUE" => Some(NamedColor::Blue),
        "GREEN" => Some(NamedColor::Green),
        "AQUA" => Some(NamedColor::Aqua),
        "RED" => Some(NamedColor::Red),
        "LIGHT_PURPLE" => Some(NamedColor::LightPurple),
        "YELLOW" => Some(NamedColor::Yellow),
        "WHITE" => Some(NamedColor::White),
        _ => None,
    }
}


pub const BAR_COLOR: Rgba<u8> = Rgba([50, 50, 55, 220]);


pub fn draw_progress_bar(
    ctx: &mut DrawContext,
    x: u32, y: u32, width: u32, height: u32,
    radius: u32, fill_frac: f64,
    bg: Rgba<u8>, fill: Rgba<u8>,
) {
    let (cw, ch) = ctx.buffer.dimensions();
    let r = radius.min(width / 2).min(height / 2);
    let fill_w = (fill_frac.clamp(0.0, 1.0) * width as f64).round() as u32;
    for py in 0..height {
        for px in 0..width {
            let abs_x = (ctx.x + x as i32 + px as i32) as u32;
            let abs_y = (ctx.y + y as i32 + py as i32) as u32;
            if abs_x >= cw || abs_y >= ch { continue; }
            if is_outside_rounded(px, py, width, height, r) { continue; }
            let color = if px < fill_w { fill } else { bg };
            let base = *ctx.buffer.get_pixel(abs_x, abs_y);
            ctx.buffer.put_pixel(abs_x, abs_y, blend(base, color));
        }
    }
}


fn is_outside_rounded(px: u32, py: u32, w: u32, h: u32, r: u32) -> bool {
    if r == 0 { return false; }
    if px < r && py < r {
        let (dx, dy) = (r - px, r - py);
        return dx * dx + dy * dy > r * r;
    }
    if px >= w - r && py < r {
        let (dx, dy) = (px - (w - r - 1), r - py);
        return dx * dx + dy * dy > r * r;
    }
    if px < r && py >= h - r {
        let (dx, dy) = (r - px, py - (h - r - 1));
        return dx * dx + dy * dy > r * r;
    }
    if px >= w - r && py >= h - r {
        let (dx, dy) = (px - (w - r - 1), py - (h - r - 1));
        return dx * dx + dy * dy > r * r;
    }
    false
}
