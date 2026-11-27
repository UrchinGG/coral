use mctext::{MCText, NamedColor, Span, TextColor, strip_codes};

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

const WHITE: u32 = 0xFFFFFF;
const MISDECODED_FORMAT_SIGN: &str = "Â§";
const FORMAT_SIGN: &str = "§";
const PROBE: char = 'x';

pub fn segments(json: &str) -> Vec<Segment> {
    let text = repair(json);
    let parsed = MCText::parse_json(&text).unwrap_or_else(|_| MCText::parse(&text));
    to_segments(&parsed, WHITE)
}

pub fn legacy_segments(text: &str, color: u32) -> Vec<Segment> {
    to_segments(&MCText::parse(&repair(text)), color)
}

pub fn plain_text(json: &str) -> String {
    let text = repair(json);
    MCText::parse_json(&text).map_or_else(|_| strip_codes(&text), |parsed| parsed.plain_text())
}

pub fn color_after(prefix: &str) -> Option<u32> {
    let probed = format!("{}{PROBE}", repair(prefix));
    MCText::parse(&probed).spans().last()?.color.map(pack)
}

pub fn legacy_color(code: char) -> Option<u32> {
    NamedColor::from_code(code).map(|color| pack(TextColor::Named(color)))
}

fn repair(text: &str) -> String {
    text.replace(MISDECODED_FORMAT_SIGN, FORMAT_SIGN)
}

fn to_segments(text: &MCText, default_color: u32) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for span in text.spans() {
        let style = style(span, default_color);
        match segments.last_mut() {
            Some(last) if last.style == style => last.text.push_str(&span.text),
            _ => segments.push(Segment {
                text: span.text.clone(),
                style,
            }),
        }
    }
    segments
}

fn style(span: &Span, default_color: u32) -> Style {
    Style {
        color: span.color.map_or(default_color, pack),
        bold: span.style.bold,
        italic: span.style.italic,
        underlined: span.style.underlined,
        strikethrough: span.style.strikethrough,
    }
}

fn pack(color: TextColor) -> u32 {
    let (r, g, b) = color.rgb();
    u32::from_be_bytes([0, r, g, b])
}
