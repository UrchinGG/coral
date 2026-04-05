use image::RgbaImage;
use mctext::MCText;

use render::canvas::{Align, BOX_BACKGROUND, CANVAS_BACKGROUND, Canvas, DrawContext, Shape, TextBox};


const IMAGE_WIDTH: u32 = 1200;
const IMAGE_HEIGHT: u32 = 900;
const HEADER_HEIGHT: u32 = 90;
const COLUMNS: u32 = 5;
const ROWS: u32 = 10;
const BOX_PADDING: u32 = 12;
const CORNER_RADIUS: u32 = 20;


pub fn render_prestiges() -> RgbaImage {
    let header = TextBox::new()
        .width(IMAGE_WIDTH).height(HEADER_HEIGHT)
        .padding(25, 25).corner_radius(CORNER_RADIUS)
        .background(BOX_BACKGROUND).scale(3.5)
        .align_x(Align::Left).align_y(Align::Center)
        .push(MCText::parse("§6\u{272B} Bedwars Prestiges 100-5000"));

    Canvas::new(IMAGE_WIDTH, IMAGE_HEIGHT)
        .background(CANVAS_BACKGROUND)
        .draw(0, 0, &header)
        .draw(0, 0, &PrestigeGrid)
        .build()
}


struct PrestigeGrid;


impl Shape for PrestigeGrid {
    fn draw(&self, ctx: &mut DrawContext) {
        let grid_height = IMAGE_HEIGHT - HEADER_HEIGHT - BOX_PADDING;
        let cell_width = (IMAGE_WIDTH - BOX_PADDING * (COLUMNS - 1)) / COLUMNS;
        let cell_height = (grid_height - BOX_PADDING * (ROWS - 1)) / ROWS;

        for i in 0..50u32 {
            let col = i / ROWS;
            let row = i % ROWS;
            let prestige = (col * 1000) + ((row + 1) * 100);

            let x = if col > 0 { col * (cell_width + BOX_PADDING) } else { 0 };
            let y = HEADER_HEIGHT + BOX_PADDING
                + if row > 0 { row * (cell_height + BOX_PADDING) } else { 0 };

            let star = prestige_star(prestige);
            let colored = build_prestige_text(
                &format!("[{}{}]", prestige, star),
                prestige_colors(prestige),
            );

            TextBox::new()
                .width(cell_width).height(cell_height)
                .padding(8, 8).corner_radius(CORNER_RADIUS)
                .background(BOX_BACKGROUND).scale(3.5)
                .align_x(Align::Center).align_y(Align::Center)
                .push(colored)
                .draw(&mut ctx.at(x as i32, y as i32));
        }
    }

    fn size(&self) -> (u32, u32) { (IMAGE_WIDTH, IMAGE_HEIGHT) }
}


pub fn prestige_colors(level: u32) -> &'static [&'static str] {
    match level {
        0..=99 => &["7", "7", "7", "7", "7", "7"],
        100..=199 => &["f", "f", "f", "f", "f", "f"],
        200..=299 => &["6", "6", "6", "6", "6", "6"],
        300..=399 => &["b", "b", "b", "b", "b", "b"],
        400..=499 => &["2", "2", "2", "2", "2", "2"],
        500..=599 => &["3", "3", "3", "3", "3", "3"],
        600..=699 => &["4", "4", "4", "4", "4", "4"],
        700..=799 => &["d", "d", "d", "d", "d", "d"],
        800..=899 => &["9", "9", "9", "9", "9", "9"],
        900..=999 => &["5", "5", "5", "5", "5", "5"],
        1000..=1099 => &["c", "6", "e", "a", "b", "d", "5"],
        1100..=1199 => &["7", "f", "f", "f", "f", "7", "7"],
        1200..=1299 => &["7", "e", "e", "e", "e", "6", "7"],
        1300..=1399 => &["7", "b", "b", "b", "b", "3", "7"],
        1400..=1499 => &["7", "a", "a", "a", "a", "2", "7"],
        1500..=1599 => &["7", "3", "3", "3", "3", "9", "7"],
        1600..=1699 => &["7", "c", "c", "c", "c", "4", "7"],
        1700..=1799 => &["7", "d", "d", "d", "d", "5", "7"],
        1800..=1899 => &["7", "9", "9", "9", "9", "1", "7"],
        1900..=1999 => &["7", "5", "5", "5", "5", "8", "7"],
        2000..=2099 => &["8", "7", "f", "f", "7", "7", "8"],
        2100..=2199 => &["f", "f", "e", "e", "6", "6", "6"],
        2200..=2299 => &["6", "6", "f", "f", "b", "3", "3"],
        2300..=2399 => &["5", "5", "d", "d", "6", "e", "e"],
        2400..=2499 => &["b", "b", "f", "f", "7", "7", "8"],
        2500..=2599 => &["f", "f", "a", "a", "2", "2", "2"],
        2600..=2699 => &["4", "4", "c", "c", "d", "d", "d"],
        2700..=2799 => &["e", "e", "f", "f", "8", "8", "8"],
        2800..=2899 => &["a", "a", "2", "2", "6", "6", "e"],
        2900..=2999 => &["b", "b", "3", "3", "9", "9", "1"],
        3000..=3099 => &["e", "e", "6", "6", "c", "c", "4"],
        3100..=3199 => &["9", "9", "3", "3", "6", "6", "e"],
        3200..=3299 => &["c", "4", "7", "7", "4", "c", "c"],
        3300..=3399 => &["9", "9", "9", "d", "c", "c", "4"],
        3400..=3499 => &["2", "a", "d", "d", "5", "5", "2"],
        3500..=3599 => &["c", "c", "4", "4", "2", "a", "a"],
        3600..=3699 => &["a", "a", "a", "b", "9", "9", "1"],
        3700..=3799 => &["4", "4", "c", "c", "b", "3", "3"],
        3800..=3899 => &["1", "1", "9", "5", "5", "d", "1"],
        3900..=3999 => &["c", "c", "a", "a", "3", "9", "9"],
        4000..=4099 => &["5", "5", "c", "c", "6", "6", "e"],
        4100..=4199 => &["e", "e", "6", "c", "d", "d", "5"],
        4200..=4299 => &["1", "9", "3", "b", "f", "7", "7"],
        4300..=4399 => &["0", "5", "8", "8", "5", "5", "0"],
        4400..=4499 => &["2", "2", "a", "e", "6", "5", "d"],
        4500..=4599 => &["f", "f", "b", "b", "3", "3", "3"],
        4600..=4699 => &["3", "b", "e", "e", "6", "d", "5"],
        4700..=4799 => &["f", "4", "c", "c", "9", "1", "9"],
        4800..=4899 => &["5", "5", "c", "6", "e", "b", "3"],
        4900..=4999 => &["2", "a", "f", "f", "a", "a", "2"],
        _ => &["4", "4", "5", "9", "9", "1", "0"],
    }
}


pub fn prestige_star(level: u32) -> &'static str {
    match level {
        0..=1099 => "\u{272b}",
        1100..=2099 => "\u{272a}",
        2100..=3099 => "\u{269d}",
        _ => "\u{2725}",
    }
}


pub fn build_prestige_text(text: &str, colors: &[&str]) -> MCText {
    let mut encoded = String::new();
    for (i, ch) in text.chars().enumerate() {
        let code = colors.get(i).copied().unwrap_or("f");
        encoded.push('\u{00a7}');
        encoded.push_str(code);
        encoded.push(ch);
    }
    MCText::parse(&encoded)
}
