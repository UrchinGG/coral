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
        .push(MCText::parse("§6\u{272B} Bed Wars Prestiges 100-10000"));

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
    colors: &'static [&'static str],
}

impl Scheme {
    fn color(&self, index: usize) -> &'static str {
        self.colors[index.min(self.colors.len() - 1)]
    }

    fn open(&self) -> &'static str {
        self.color(0)
    }

    fn digit(&self, i: usize) -> &'static str {
        self.color(1 + i.min(3))
    }

    fn icon(&self) -> &'static str {
        self.color(5)
    }

    fn close(&self) -> &'static str {
        self.color(6)
    }
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
    push_colored(&mut text, scheme.open(), open);
    for (i, digit) in level.to_string().chars().enumerate() {
        push_colored(&mut text, scheme.digit(i), &digit.to_string());
    }
    push_colored(&mut text, scheme.icon(), star);
    push_colored(&mut text, scheme.close(), close);
    MCText::parse(&text)
}

pub fn prestige_color_codes() -> String {
    (1..=(COLUMNS * ROWS))
        .map(|n| {
            let level = n * 100;
            let name = pretty_scheme_name(scheme_for_level(level).name);
            format!("{name} `{}`", prestige_code(level))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn pretty_scheme_name(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn prestige_code(level: u32) -> String {
    let scheme = scheme_for_level(level);
    let star = star_for_level(level);

    let mut segments: Vec<(&str, String)> = vec![(scheme.open(), "[".to_string())];
    for (i, digit) in level.to_string().chars().enumerate() {
        segments.push((scheme.digit(i), digit.to_string()));
    }
    segments.push((scheme.icon(), star.to_string()));
    segments.push((scheme.close(), "]".to_string()));

    let mut out = String::new();
    let mut current = "";
    for (color, content) in &segments {
        if *color != current {
            out.push('\u{00a7}');
            out.push_str(color);
            current = color;
        }
        out.push_str(content);
    }
    out
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
    Scheme { name: "none", colors: &["7"] },
    Scheme { name: "iron", colors: &["f"] },
    Scheme { name: "gold", colors: &["6"] },
    Scheme { name: "diamond", colors: &["b"] },
    Scheme { name: "emerald", colors: &["2"] },
    Scheme { name: "sapphire", colors: &["3"] },
    Scheme { name: "ruby", colors: &["4"] },
    Scheme { name: "crystal", colors: &["d"] },
    Scheme { name: "opal", colors: &["9"] },
    Scheme { name: "amethyst", colors: &["5"] },
    Scheme { name: "rainbow", colors: &["c", "6", "e", "a", "b", "d", "5"] },
    Scheme { name: "iron_prime", colors: &["7", "f", "f", "f", "f", "7", "7"] },
    Scheme { name: "gold_prime", colors: &["7", "e", "e", "e", "e", "6", "7"] },
    Scheme { name: "diamond_prime", colors: &["7", "b", "b", "b", "b", "3", "7"] },
    Scheme { name: "emerald_prime", colors: &["7", "a", "a", "a", "a", "2", "7"] },
    Scheme { name: "sapphire_prime", colors: &["7", "3", "3", "3", "3", "9", "7"] },
    Scheme { name: "ruby_prime", colors: &["7", "c", "c", "c", "c", "4", "7"] },
    Scheme { name: "crystal_prime", colors: &["7", "d", "d", "d", "d", "5", "7"] },
    Scheme { name: "opal_prime", colors: &["7", "9", "9", "9", "9", "1", "7"] },
    Scheme { name: "amethyst_prime", colors: &["7", "5", "5", "5", "5", "8", "7"] },
    Scheme { name: "mirror", colors: &["8", "7", "f", "f", "7", "7", "8"] },
    Scheme { name: "light", colors: &["f", "f", "e", "e", "6", "6", "6"] },
    Scheme { name: "dawn", colors: &["6", "6", "f", "f", "b", "3", "3"] },
    Scheme { name: "dusk", colors: &["5", "5", "d", "d", "6", "e", "e"] },
    Scheme { name: "air", colors: &["b", "b", "f", "f", "7", "7", "8"] },
    Scheme { name: "wind", colors: &["f", "f", "a", "a", "2", "2", "2"] },
    Scheme { name: "nebula", colors: &["4", "4", "c", "c", "d", "d", "5"] },
    Scheme { name: "thunder", colors: &["e", "e", "f", "f", "8", "8", "8"] },
    Scheme { name: "earth", colors: &["a", "a", "2", "2", "6", "6", "e"] },
    Scheme { name: "water", colors: &["b", "b", "3", "3", "9", "9", "1"] },
    Scheme { name: "fire", colors: &["e", "e", "6", "6", "c", "c", "4"] },
    Scheme { name: "sunrise", colors: &["9", "9", "3", "3", "6", "6", "e"] },
    Scheme { name: "eclipse", colors: &["c", "4", "7", "7", "4", "c", "c"] },
    Scheme { name: "gamma", colors: &["9", "9", "9", "d", "c", "c", "4"] },
    Scheme { name: "majestic", colors: &["2", "a", "d", "d", "5", "5", "2"] },
    Scheme { name: "andesine", colors: &["c", "c", "4", "4", "2", "a", "a"] },
    Scheme { name: "marine", colors: &["a", "a", "a", "b", "9", "9", "1"] },
    Scheme { name: "element", colors: &["4", "4", "c", "c", "b", "3", "3"] },
    Scheme { name: "galaxy", colors: &["1", "1", "9", "5", "5", "d", "1"] },
    Scheme { name: "atomic", colors: &["c", "c", "a", "a", "3", "9", "9"] },
    Scheme { name: "sunset", colors: &["5", "5", "c", "c", "6", "6", "e"] },
    Scheme { name: "time", colors: &["e", "e", "6", "c", "d", "d", "5"] },
    Scheme { name: "winter", colors: &["1", "9", "3", "b", "f", "7", "7"] },
    Scheme { name: "obsidian", colors: &["0", "5", "8", "8", "5", "5", "0"] },
    Scheme { name: "spring", colors: &["2", "2", "a", "e", "6", "5", "d"] },
    Scheme { name: "ice", colors: &["f", "f", "b", "b", "3", "3", "3"] },
    Scheme { name: "summer", colors: &["3", "b", "e", "6", "6", "d", "5"] },
    Scheme { name: "spinel", colors: &["f", "4", "c", "c", "9", "1", "9"] },
    Scheme { name: "autumn", colors: &["5", "5", "c", "6", "6", "b", "3"] },
    Scheme { name: "mystic", colors: &["2", "a", "f", "f", "f", "a", "2"] },
    Scheme { name: "eternal", colors: &["4", "4", "5", "9", "9", "1", "0"] },
    Scheme { name: "burnout", colors: &["4", "c", "c", "6", "e", "f", "4"] },
    Scheme { name: "cooldown", colors: &["1", "9", "3", "b", "f", "e", "1"] },
    Scheme { name: "obliteration", colors: &["5", "d", "e", "f", "e", "d", "5"] },
    Scheme { name: "ender", colors: &["3", "a", "2", "8", "2", "a", "3"] },
    Scheme { name: "brust", colors: &["2", "a", "e", "f", "b", "d", "5"] },
    Scheme { name: "comical", colors: &["4", "c", "e", "f", "e", "c", "4"] },
    Scheme { name: "lusterlost", colors: &["4", "6", "2", "3", "9", "5", "8"] },
    Scheme { name: "maelstrom", colors: &["5", "c", "6", "f", "b", "3", "9"] },
    Scheme { name: "time_undone", colors: &["7", "0", "8", "7", "f", "f", "7"] },
    Scheme { name: "umbrella", colors: &["c", "f", "f", "f", "f", "c", "f"] },
    Scheme { name: "luminous", colors: &["6", "e", "f", "f", "f", "b", "3"] },
    Scheme { name: "tortilla", colors: &["e", "f", "e", "6", "6", "f", "e"] },
    Scheme { name: "corn", colors: &["a", "e", "e", "e", "e", "a", "2"] },
    Scheme { name: "bittersweet", colors: &["b", "b", "c", "c", "c", "a", "a"] },
    Scheme { name: "sweetsour", colors: &["3", "3", "a", "a", "f", "a", "3"] },
    Scheme { name: "pop", colors: &["9", "d", "d", "d", "d", "b", "9"] },
    Scheme { name: "bubblegum", colors: &["5", "d", "d", "d", "d", "f", "5"] },
    Scheme { name: "contrast", colors: &["0", "6", "6", "e", "e", "f", "f"] },
    Scheme { name: "blended", colors: &["a", "a", "a", "a", "2", "2", "8"] },
    Scheme { name: "allay", colors: &["3", "b", "b", "b", "b", "f", "3"] },
    Scheme { name: "blaze", colors: &["4", "c", "6", "e", "c", "6", "e"] },
    Scheme { name: "creeper", colors: &["2", "a", "f", "2", "a", "f", "8"] },
    Scheme { name: "drowned", colors: &["2", "3", "3", "b", "b", "a", "2"] },
    Scheme { name: "enderman", colors: &["8", "8", "8", "8", "8", "d", "8"] },
    Scheme { name: "frog", colors: &["6", "6", "2", "2", "f", "f", "f"] },
    Scheme { name: "ghast", colors: &["f", "f", "f", "7", "7", "c", "8"] },
    Scheme { name: "hoglin", colors: &["d", "c", "c", "c", "c", "6", "d"] },
    Scheme { name: "iron_golem", colors: &["8", "7", "f", "f", "f", "e", "8"] },
    Scheme { name: "jerry", colors: &["6", "f", "2", "6", "2", "f", "6"] },
    Scheme { name: "kringle", colors: &["2", "a", "a", "a", "c", "4", "2"] },
    Scheme { name: "liquid", colors: &["8", "7", "f", "b", "3", "9", "1"] },
    Scheme { name: "mint", colors: &["f", "f", "f", "f", "f", "a", "f"] },
    Scheme { name: "neglected", colors: &["8", "8", "4", "4", "c", "c", "8"] },
    Scheme { name: "onion", colors: &["f", "d", "d", "d", "a", "a", "f"] },
    Scheme { name: "poser", colors: &["3", "6", "6", "6", "6", "e", "3"] },
    Scheme { name: "quartz", colors: &["d", "f", "f", "f", "f", "e", "d"] },
    Scheme { name: "rich", colors: &["8", "6", "6", "6", "6", "6", "8"] },
    Scheme { name: "sanguine", colors: &["4", "4", "4", "c", "c", "f", "f"] },
    Scheme { name: "titanic", colors: &["9", "b", "b", "b", "3", "3", "9"] },
    Scheme { name: "unorthodox", colors: &["d", "d", "d", "d", "d", "5", "8"] },
    Scheme { name: "volcanic", colors: &["0", "c", "6", "6", "c", "c", "4"] },
    Scheme { name: "weeping_cherry", colors: &["2", "d", "d", "d", "d", "a", "2"] },
    Scheme { name: "x-ray", colors: &["f", "8", "8", "8", "8", "f", "f"] },
    Scheme { name: "yearn", colors: &["e", "6", "4", "8", "8", "8", "8"] },
    Scheme { name: "zebra", colors: &["0", "0", "8", "8", "7", "7", "f"] },
    Scheme { name: "caution", colors: &["e", "e", "e", "0", "0", "e", "0"] },
    Scheme { name: "indescribable", colors: &["d", "d", "d", "e", "e", "b", "e"] },
    Scheme { name: "forgotten", colors: &["0", "8", "8", "8", "8", "8", "0"] },
    Scheme { name: "fuse", colors: &["8", "7", "f", "f", "f", "e", "f"] },
    Scheme { name: "prestigious", colors: &["9", "b", "f", "f", "f", "c", "4"] },
];
