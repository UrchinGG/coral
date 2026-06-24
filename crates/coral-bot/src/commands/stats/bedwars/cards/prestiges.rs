use image::RgbaImage;
use mctext::MCText;
use rand::rngs::ThreadRng;
use rand::seq::SliceRandom;

use hypixel::{CosmeticSlot, Cosmetics};
use render::canvas::{
    Align, BOX_BACKGROUND, CANVAS_BACKGROUND, Canvas, DrawContext, Shape, TextBox,
};

const IMAGE_WIDTH: u32 = 2400;
const IMAGE_HEIGHT: u32 = 900;
const HEADER_HEIGHT: u32 = 90;
const COLUMNS: u32 = 10;
const ROWS: u32 = 10;
const BOX_PADDING: u32 = 12;
const CORNER_RADIUS: u32 = 20;

pub fn render_prestiges() -> RgbaImage {
    let header = TextBox::new()
        .width(IMAGE_WIDTH)
        .height(HEADER_HEIGHT)
        .padding(25, 25)
        .corner_radius(CORNER_RADIUS)
        .background(BOX_BACKGROUND)
        .scale(3.5)
        .align_x(Align::Left)
        .align_y(Align::Center)
        .push(MCText::parse("§6\u{272B} Bedwars Prestiges 100-10000"));

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

        for i in 0..(COLUMNS * ROWS) {
            let col = i / ROWS;
            let row = i % ROWS;
            let prestige = (col * 1000) + ((row + 1) * 100);

            let x = col * (cell_width + BOX_PADDING);
            let y = HEADER_HEIGHT + BOX_PADDING + row * (cell_height + BOX_PADDING);

            TextBox::new()
                .width(cell_width)
                .height(cell_height)
                .padding(8, 8)
                .corner_radius(CORNER_RADIUS)
                .background(BOX_BACKGROUND)
                .scale(3.5)
                .align_x(Align::Center)
                .align_y(Align::Center)
                .push(render_prestige(prestige, &Cosmetics::default()))
                .draw(&mut ctx.at(x as i32, y as i32));
        }
    }

    fn size(&self) -> (u32, u32) {
        (IMAGE_WIDTH, IMAGE_HEIGHT)
    }
}

struct Scheme {
    name: &'static str,
    open: &'static str,
    digits: [&'static str; 4],
    icon: &'static str,
    close: &'static str,
}

pub fn render_prestige(level: u32, cosmetics: &Cosmetics) -> MCText {
    let scheme = cosmetics
        .scheme
        .active
        .as_deref()
        .and_then(scheme_by_name)
        .unwrap_or_else(|| scheme_for_level(level));
    let star = cosmetics
        .star
        .active
        .as_deref()
        .and_then(star_by_id)
        .unwrap_or_else(|| star_for_level(level));
    let (open, close) = cosmetics
        .bracket
        .active
        .as_deref()
        .and_then(bracket_by_id)
        .unwrap_or(("[", "]"));

    let mut text = String::new();
    push_colored(&mut text, scheme.open, open);
    for (i, digit) in level.to_string().chars().enumerate() {
        let color = scheme.digits.get(i).copied().unwrap_or(scheme.digits[3]);
        push_colored(&mut text, color, &digit.to_string());
    }
    push_colored(&mut text, scheme.icon, star);
    push_colored(&mut text, scheme.close, close);
    MCText::parse(&text)
}

pub fn resolve_cosmetics(cosmetics: &Cosmetics) -> Cosmetics {
    let mut rng = rand::thread_rng();
    let all_schemes: Vec<String> = SCHEMES
        .iter()
        .map(|scheme| format!("prestige_scheme_{}", scheme.name))
        .collect();
    let stars: Vec<String> = STARS.iter().map(|(id, _)| id.to_string()).collect();
    let brackets: Vec<String> = BRACKETS.iter().map(|(id, _)| id.to_string()).collect();

    Cosmetics {
        scheme: resolve_slot(
            &cosmetics.scheme,
            "prestige_scheme_",
            &all_schemes,
            &mut rng,
        ),
        star: resolve_slot(&cosmetics.star, "star_", &stars, &mut rng),
        bracket: resolve_slot(&cosmetics.bracket, "prestige_bracket_", &brackets, &mut rng),
    }
}

fn resolve_slot(
    slot: &CosmeticSlot,
    prefix: &str,
    known: &[String],
    rng: &mut ThreadRng,
) -> CosmeticSlot {
    let active = slot.active.as_deref().map(|id| {
        let pick = match id {
            "random_cosmetic" => slot
                .unlocked
                .iter()
                .filter(|owned| known.contains(owned))
                .collect::<Vec<_>>()
                .choose(rng)
                .map(|id| id.to_string()),
            "random_favorite_cosmetic" => slot
                .favorites
                .choose(rng)
                .map(|name| format!("{prefix}{name}")),
            _ => Some(id.to_string()),
        };
        pick.unwrap_or_else(|| id.to_string())
    });
    CosmeticSlot {
        active,
        ..Default::default()
    }
}

pub fn prestige_accent(level: u32, cosmetics: &Cosmetics) -> (&'static str, &'static str) {
    let scheme = cosmetics
        .scheme
        .active
        .as_deref()
        .and_then(scheme_by_name)
        .unwrap_or_else(|| scheme_for_level(level));
    let star = cosmetics
        .star
        .active
        .as_deref()
        .and_then(star_by_id)
        .unwrap_or_else(|| star_for_level(level));
    (scheme.icon, star)
}

fn push_colored(text: &mut String, color: &str, content: &str) {
    text.push('\u{00a7}');
    text.push_str(color);
    text.push_str(content);
}

fn scheme_for_level(level: u32) -> &'static Scheme {
    &SCHEMES[(level / 100).min(100) as usize]
}

fn scheme_by_name(id: &str) -> Option<&'static Scheme> {
    let name = id.strip_prefix("prestige_scheme_")?;
    SCHEMES.iter().find(|scheme| scheme.name == name)
}

fn star_for_level(level: u32) -> &'static str {
    match level / 1000 {
        0 => "\u{272b}",
        1 => "\u{272a}",
        2 => "\u{269d}",
        3 => "\u{2725}",
        _ => "\u{272d}",
    }
}

static STARS: &[(&str, &str)] = &[
    ("star_black_open", "\u{272b}"),
    ("star_white_circled", "\u{272a}"),
    ("star_white_outlined", "\u{269d}"),
    ("star_four_clubs", "\u{2725}"),
    ("star_black_outlined", "\u{272d}"),
    ("star_four_pointed", "\u{2726}"),
    ("star_pinwheel", "\u{2735}"),
    ("star_hollow", "\u{2730}"),
    ("star_nautical", "\u{272f}"),
];

static BRACKETS: &[(&str, (&str, &str))] = &[
    ("prestige_bracket_none", ("[", "]")),
    ("prestige_bracket_curly", ("{", "}")),
    ("prestige_bracket_angled", ("<", ">")),
    ("prestige_bracket_parenthesis", ("(", ")")),
    (
        "prestige_bracket_double_angle_quotation_mark",
        ("\u{ab}", "\u{bb}"),
    ),
];

fn star_by_id(id: &str) -> Option<&'static str> {
    STARS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, glyph)| *glyph)
}

fn bracket_by_id(id: &str) -> Option<(&'static str, &'static str)> {
    BRACKETS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, chars)| *chars)
}

#[rustfmt::skip]
static SCHEMES: [Scheme; 101] = [
    Scheme { name: "none", open: "7", digits: ["7", "7", "7", "7"], icon: "7", close: "7" },
    Scheme { name: "iron", open: "f", digits: ["f", "f", "f", "f"], icon: "f", close: "f" },
    Scheme { name: "gold", open: "6", digits: ["6", "6", "6", "6"], icon: "6", close: "6" },
    Scheme { name: "diamond", open: "b", digits: ["b", "b", "b", "b"], icon: "b", close: "b" },
    Scheme { name: "emerald", open: "2", digits: ["2", "2", "2", "2"], icon: "2", close: "2" },
    Scheme { name: "sapphire", open: "3", digits: ["3", "3", "3", "3"], icon: "3", close: "3" },
    Scheme { name: "ruby", open: "4", digits: ["4", "4", "4", "4"], icon: "4", close: "4" },
    Scheme { name: "crystal", open: "d", digits: ["d", "d", "d", "d"], icon: "d", close: "d" },
    Scheme { name: "opal", open: "9", digits: ["9", "9", "9", "9"], icon: "9", close: "9" },
    Scheme { name: "amethyst", open: "5", digits: ["5", "5", "5", "5"], icon: "5", close: "5" },
    Scheme { name: "rainbow", open: "c", digits: ["6", "e", "a", "b"], icon: "d", close: "5" },
    Scheme { name: "iron_prime", open: "7", digits: ["f", "f", "f", "f"], icon: "7", close: "7" },
    Scheme { name: "gold_prime", open: "7", digits: ["e", "e", "e", "e"], icon: "6", close: "7" },
    Scheme { name: "diamond_prime", open: "7", digits: ["b", "b", "b", "b"], icon: "3", close: "7" },
    Scheme { name: "emerald_prime", open: "7", digits: ["a", "a", "a", "a"], icon: "2", close: "7" },
    Scheme { name: "sapphire_prime", open: "7", digits: ["3", "3", "3", "3"], icon: "9", close: "7" },
    Scheme { name: "ruby_prime", open: "7", digits: ["c", "c", "c", "c"], icon: "4", close: "7" },
    Scheme { name: "crystal_prime", open: "7", digits: ["d", "d", "d", "d"], icon: "5", close: "7" },
    Scheme { name: "opal_prime", open: "7", digits: ["9", "9", "9", "9"], icon: "1", close: "7" },
    Scheme { name: "amethyst_prime", open: "7", digits: ["5", "5", "5", "5"], icon: "8", close: "7" },
    Scheme { name: "mirror", open: "8", digits: ["7", "f", "f", "7"], icon: "7", close: "8" },
    Scheme { name: "light", open: "f", digits: ["f", "e", "e", "6"], icon: "6", close: "6" },
    Scheme { name: "dawn", open: "6", digits: ["6", "f", "f", "b"], icon: "3", close: "3" },
    Scheme { name: "dusk", open: "5", digits: ["5", "d", "d", "6"], icon: "e", close: "e" },
    Scheme { name: "air", open: "b", digits: ["b", "f", "f", "7"], icon: "7", close: "8" },
    Scheme { name: "wind", open: "f", digits: ["f", "a", "a", "2"], icon: "2", close: "2" },
    Scheme { name: "nebula", open: "4", digits: ["4", "c", "c", "d"], icon: "d", close: "5" },
    Scheme { name: "thunder", open: "e", digits: ["e", "f", "f", "8"], icon: "8", close: "8" },
    Scheme { name: "earth", open: "a", digits: ["a", "2", "2", "6"], icon: "6", close: "e" },
    Scheme { name: "water", open: "b", digits: ["b", "3", "3", "9"], icon: "9", close: "1" },
    Scheme { name: "fire", open: "e", digits: ["e", "6", "6", "c"], icon: "c", close: "4" },
    Scheme { name: "sunrise", open: "9", digits: ["9", "3", "3", "6"], icon: "6", close: "e" },
    Scheme { name: "eclipse", open: "c", digits: ["4", "7", "7", "4"], icon: "c", close: "c" },
    Scheme { name: "gamma", open: "9", digits: ["9", "9", "d", "c"], icon: "c", close: "4" },
    Scheme { name: "majestic", open: "2", digits: ["a", "d", "d", "5"], icon: "5", close: "2" },
    Scheme { name: "andesine", open: "c", digits: ["c", "4", "4", "2"], icon: "a", close: "a" },
    Scheme { name: "marine", open: "a", digits: ["a", "a", "b", "9"], icon: "9", close: "1" },
    Scheme { name: "element", open: "4", digits: ["4", "c", "c", "b"], icon: "3", close: "3" },
    Scheme { name: "galaxy", open: "1", digits: ["1", "9", "5", "5"], icon: "d", close: "1" },
    Scheme { name: "atomic", open: "c", digits: ["c", "a", "a", "3"], icon: "9", close: "9" },
    Scheme { name: "sunset", open: "5", digits: ["5", "c", "c", "6"], icon: "6", close: "e" },
    Scheme { name: "time", open: "e", digits: ["e", "6", "c", "d"], icon: "d", close: "5" },
    Scheme { name: "winter", open: "1", digits: ["9", "3", "b", "f"], icon: "7", close: "7" },
    Scheme { name: "obsidian", open: "0", digits: ["5", "8", "8", "5"], icon: "5", close: "0" },
    Scheme { name: "spring", open: "2", digits: ["2", "a", "e", "6"], icon: "5", close: "d" },
    Scheme { name: "ice", open: "f", digits: ["f", "b", "b", "3"], icon: "3", close: "3" },
    Scheme { name: "summer", open: "3", digits: ["b", "e", "6", "6"], icon: "d", close: "5" },
    Scheme { name: "spinel", open: "f", digits: ["4", "c", "c", "9"], icon: "1", close: "9" },
    Scheme { name: "autumn", open: "5", digits: ["5", "c", "6", "6"], icon: "b", close: "3" },
    Scheme { name: "mystic", open: "2", digits: ["a", "f", "f", "f"], icon: "a", close: "2" },
    Scheme { name: "eternal", open: "4", digits: ["4", "5", "9", "9"], icon: "1", close: "0" },
    Scheme { name: "burnout", open: "4", digits: ["c", "c", "6", "e"], icon: "f", close: "4" },
    Scheme { name: "cooldown", open: "1", digits: ["9", "3", "b", "f"], icon: "e", close: "1" },
    Scheme { name: "obliteration", open: "5", digits: ["d", "e", "f", "e"], icon: "d", close: "5" },
    Scheme { name: "ender", open: "3", digits: ["a", "2", "8", "2"], icon: "a", close: "3" },
    Scheme { name: "brust", open: "2", digits: ["a", "e", "f", "b"], icon: "d", close: "5" },
    Scheme { name: "comical", open: "4", digits: ["c", "e", "f", "e"], icon: "c", close: "4" },
    Scheme { name: "lusterlost", open: "4", digits: ["6", "2", "3", "9"], icon: "5", close: "8" },
    Scheme { name: "maelstrom", open: "5", digits: ["c", "6", "f", "b"], icon: "3", close: "9" },
    Scheme { name: "time_undone", open: "7", digits: ["0", "8", "7", "f"], icon: "f", close: "7" },
    Scheme { name: "umbrella", open: "c", digits: ["f", "f", "f", "f"], icon: "c", close: "f" },
    Scheme { name: "luminous", open: "6", digits: ["e", "f", "f", "f"], icon: "b", close: "3" },
    Scheme { name: "tortilla", open: "e", digits: ["f", "e", "6", "6"], icon: "f", close: "e" },
    Scheme { name: "corn", open: "a", digits: ["e", "e", "e", "e"], icon: "a", close: "2" },
    Scheme { name: "bittersweet", open: "b", digits: ["b", "c", "c", "c"], icon: "a", close: "a" },
    Scheme { name: "sweetsour", open: "3", digits: ["3", "a", "a", "f"], icon: "a", close: "3" },
    Scheme { name: "pop", open: "9", digits: ["d", "d", "d", "d"], icon: "b", close: "9" },
    Scheme { name: "bubblegum", open: "5", digits: ["d", "d", "d", "d"], icon: "f", close: "5" },
    Scheme { name: "contrast", open: "0", digits: ["6", "6", "e", "e"], icon: "f", close: "f" },
    Scheme { name: "blended", open: "a", digits: ["a", "a", "a", "2"], icon: "2", close: "8" },
    Scheme { name: "allay", open: "3", digits: ["b", "b", "b", "b"], icon: "f", close: "3" },
    Scheme { name: "blaze", open: "4", digits: ["c", "6", "e", "c"], icon: "6", close: "e" },
    Scheme { name: "creeper", open: "2", digits: ["a", "f", "2", "a"], icon: "f", close: "8" },
    Scheme { name: "drowned", open: "2", digits: ["3", "3", "b", "b"], icon: "a", close: "2" },
    Scheme { name: "enderman", open: "8", digits: ["8", "8", "8", "8"], icon: "d", close: "8" },
    Scheme { name: "frog", open: "6", digits: ["6", "2", "2", "f"], icon: "f", close: "f" },
    Scheme { name: "ghast", open: "f", digits: ["f", "f", "7", "7"], icon: "c", close: "8" },
    Scheme { name: "hoglin", open: "d", digits: ["c", "c", "c", "c"], icon: "6", close: "d" },
    Scheme { name: "iron_golem", open: "8", digits: ["7", "f", "f", "f"], icon: "e", close: "8" },
    Scheme { name: "jerry", open: "6", digits: ["f", "2", "6", "2"], icon: "f", close: "6" },
    Scheme { name: "kringle", open: "2", digits: ["a", "a", "a", "c"], icon: "4", close: "2" },
    Scheme { name: "liquid", open: "8", digits: ["7", "f", "b", "3"], icon: "9", close: "1" },
    Scheme { name: "mint", open: "f", digits: ["f", "f", "f", "f"], icon: "a", close: "f" },
    Scheme { name: "neglected", open: "8", digits: ["8", "4", "4", "c"], icon: "c", close: "8" },
    Scheme { name: "onion", open: "f", digits: ["d", "d", "d", "a"], icon: "a", close: "f" },
    Scheme { name: "poser", open: "3", digits: ["6", "6", "6", "6"], icon: "e", close: "3" },
    Scheme { name: "quartz", open: "d", digits: ["f", "f", "f", "f"], icon: "e", close: "d" },
    Scheme { name: "rich", open: "8", digits: ["6", "6", "6", "6"], icon: "6", close: "8" },
    Scheme { name: "sanguine", open: "4", digits: ["4", "4", "c", "c"], icon: "f", close: "f" },
    Scheme { name: "titanic", open: "9", digits: ["b", "b", "b", "3"], icon: "3", close: "9" },
    Scheme { name: "unorthodox", open: "d", digits: ["d", "d", "d", "d"], icon: "5", close: "8" },
    Scheme { name: "volcanic", open: "0", digits: ["c", "6", "6", "c"], icon: "c", close: "4" },
    Scheme { name: "weeping_cherry", open: "2", digits: ["d", "d", "d", "d"], icon: "a", close: "2" },
    Scheme { name: "x-ray", open: "f", digits: ["8", "8", "8", "8"], icon: "f", close: "f" },
    Scheme { name: "yearn", open: "e", digits: ["6", "4", "8", "8"], icon: "8", close: "8" },
    Scheme { name: "zebra", open: "0", digits: ["0", "8", "8", "7"], icon: "7", close: "f" },
    Scheme { name: "caution", open: "e", digits: ["e", "e", "0", "0"], icon: "e", close: "0" },
    Scheme { name: "indescribable", open: "d", digits: ["d", "d", "e", "e"], icon: "b", close: "e" },
    Scheme { name: "forgotten", open: "0", digits: ["8", "8", "8", "8"], icon: "8", close: "0" },
    Scheme { name: "fuse", open: "8", digits: ["7", "f", "f", "f"], icon: "e", close: "f" },
    Scheme { name: "prestigious", open: "9", digits: ["b", "f", "f", "f"], icon: "c", close: "4" },
];
