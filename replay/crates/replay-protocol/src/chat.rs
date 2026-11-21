use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    pub color: u32,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Segment {
    pub text: String,
    pub style: Style,
}

const PLAIN: Style = Style {
    color: 0xFFFFFF,
    bold: false,
    italic: false,
    underlined: false,
    strikethrough: false,
};

const COLORS: [(char, &str, u32); 16] = [
    ('0', "black", 0x000000),
    ('1', "dark_blue", 0x0000AA),
    ('2', "dark_green", 0x00AA00),
    ('3', "dark_aqua", 0x00AAAA),
    ('4', "dark_red", 0xAA0000),
    ('5', "dark_purple", 0xAA00AA),
    ('6', "gold", 0xFFAA00),
    ('7', "gray", 0xAAAAAA),
    ('8', "dark_gray", 0x555555),
    ('9', "blue", 0x5555FF),
    ('a', "green", 0x55FF55),
    ('b', "aqua", 0x55FFFF),
    ('c', "red", 0xFF5555),
    ('d', "light_purple", 0xFF55FF),
    ('e', "yellow", 0xFFFF55),
    ('f', "white", 0xFFFFFF),
];

pub fn flatten_text(json: &str) -> String {
    serde_json::from_str(json).map_or_else(|_| json.to_owned(), |component| flatten(&component))
}

fn flatten(component: &Value) -> String {
    match component {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().map(flatten).collect(),
        Value::Object(fields) => {
            let text = fields
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            text.to_owned() + &fields.get("extra").map(flatten).unwrap_or_default()
        }
        _ => String::new(),
    }
}

pub fn segments(json: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    match serde_json::from_str(json) {
        Ok(component) => push_component(&mut segments, &component, PLAIN),
        Err(_) => push_legacy(&mut segments, json, PLAIN),
    }
    segments
}

pub fn legacy_color(code: char) -> Option<u32> {
    let code = code.to_ascii_lowercase();
    COLORS
        .iter()
        .find(|(legacy, ..)| *legacy == code)
        .map(|&(.., color)| color)
}

pub fn legacy_segments(text: &str, color: u32) -> Vec<Segment> {
    let mut segments = Vec::new();
    push_legacy(&mut segments, text, Style { color, ..PLAIN });
    segments
}

fn push_component(segments: &mut Vec<Segment>, component: &Value, inherited: Style) {
    match component {
        Value::String(text) => push_legacy(segments, text, inherited),
        Value::Array(parts) => parts
            .iter()
            .for_each(|part| push_component(segments, part, inherited)),
        Value::Object(fields) => {
            let flag = |name, inherited| {
                fields
                    .get(name)
                    .and_then(Value::as_bool)
                    .unwrap_or(inherited)
            };
            let color = fields.get("color").and_then(Value::as_str);
            let style = Style {
                color: COLORS
                    .iter()
                    .find(|(_, name, _)| Some(*name) == color)
                    .map_or(inherited.color, |&(.., rgb)| rgb),
                bold: flag("bold", inherited.bold),
                italic: flag("italic", inherited.italic),
                underlined: flag("underlined", inherited.underlined),
                strikethrough: flag("strikethrough", inherited.strikethrough),
            };
            if let Some(text) = fields.get("text").and_then(Value::as_str) {
                push_legacy(segments, text, style);
            }
            if let Some(extra) = fields.get("extra") {
                push_component(segments, extra, style);
            }
        }
        _ => {}
    }
}

fn push_legacy(segments: &mut Vec<Segment>, text: &str, mut style: Style) {
    let text = text.replace("Â§", "§");
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '§'
            && let Some(code) = characters.next()
        {
            style = apply_code(style, code.to_ascii_lowercase());
            continue;
        }
        match segments.last_mut() {
            Some(last) if last.style == style => last.text.push(character),
            _ => segments.push(Segment {
                text: character.into(),
                style,
            }),
        }
    }
}

fn apply_code(style: Style, code: char) -> Style {
    match code {
        'l' => Style {
            bold: true,
            ..style
        },
        'o' => Style {
            italic: true,
            ..style
        },
        'n' => Style {
            underlined: true,
            ..style
        },
        'm' => Style {
            strikethrough: true,
            ..style
        },
        'r' => PLAIN,
        _ => legacy_color(code).map_or(style, |color| Style { color, ..PLAIN }),
    }
}
