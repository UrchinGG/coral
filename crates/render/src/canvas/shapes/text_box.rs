use image::Rgba;
use mctext::MCText;

use super::is_outside_rounded_rect;
use crate::canvas::{
    color::{BOX_BACKGROUND, blend},
    context::DrawContext,
    shape::Shape,
    text::TextRenderer,
};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    Left,
    Right,
    Top,
    Bottom,
    #[default]
    Center,
    Spread,
}

pub struct TextBox {
    lines: Vec<MCText>,
    scale: f32,
    line_spacing: f32,
    align_x: Align,
    align_y: Align,
    shadow: bool,
    padding: (u32, u32),
    corner_radius: u32,
    background: Rgba<u8>,
    width: Option<u32>,
    height: Option<u32>,
}

impl TextBox {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scale: 1.0,
            line_spacing: 8.0,
            align_x: Align::Left,
            align_y: Align::Top,
            shadow: true,
            padding: (20, 20),
            corner_radius: 30,
            background: BOX_BACKGROUND,
            width: None,
            height: None,
        }
    }

    pub fn push(mut self, line: MCText) -> Self {
        self.lines.push(line);
        self
    }
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
    pub fn line_spacing(mut self, spacing: f32) -> Self {
        self.line_spacing = spacing;
        self
    }
    pub fn align_x(mut self, align: Align) -> Self {
        self.align_x = align;
        self
    }
    pub fn align_y(mut self, align: Align) -> Self {
        self.align_y = align;
        self
    }
    pub fn shadow(mut self, shadow: bool) -> Self {
        self.shadow = shadow;
        self
    }
    pub fn padding(mut self, x: u32, y: u32) -> Self {
        self.padding = (x, y);
        self
    }
    pub fn corner_radius(mut self, radius: u32) -> Self {
        self.corner_radius = radius;
        self
    }
    pub fn background(mut self, color: Rgba<u8>) -> Self {
        self.background = color;
        self
    }
    pub fn width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: u32) -> Self {
        self.height = Some(height);
        self
    }

    fn measure_lines(&self, renderer: &TextRenderer) -> Vec<(f32, f32)> {
        let font_size = self.scale * 16.0;
        self.lines
            .iter()
            .map(|line| renderer.measure(line, font_size))
            .collect()
    }

    fn content_size(measurements: &[(f32, f32)], line_spacing: f32) -> (f32, f32) {
        let max_width = measurements.iter().map(|(w, _)| *w).fold(0.0f32, f32::max);
        let total_height: f32 = measurements.iter().map(|(_, h)| *h).sum::<f32>()
            + measurements.len().saturating_sub(1) as f32 * line_spacing;
        (max_width, total_height)
    }
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape for TextBox {
    fn measure(&self, renderer: &TextRenderer) -> (u32, u32) {
        let measurements = self.measure_lines(renderer);
        let (content_w, content_h) =
            Self::content_size(&measurements, self.line_spacing * self.scale);
        (
            self.width
                .unwrap_or(content_w.ceil() as u32 + self.padding.0 * 2),
            self.height
                .unwrap_or(content_h.ceil() as u32 + self.padding.1 * 2),
        )
    }

    fn draw(&self, ctx: &mut DrawContext) {
        let measurements = self.measure_lines(ctx.renderer);
        let spacing = self.line_spacing * self.scale;
        let (content_w, content_h) = Self::content_size(&measurements, spacing);

        let box_w = self
            .width
            .unwrap_or(content_w.ceil() as u32 + self.padding.0 * 2);
        let box_h = self
            .height
            .unwrap_or(content_h.ceil() as u32 + self.padding.1 * 2);
        let inner_w = box_w.saturating_sub(self.padding.0 * 2);
        let inner_h = box_h.saturating_sub(self.padding.1 * 2);

        let (cw, ch) = ctx.buffer.dimensions();
        let radius = self.corner_radius.min(box_w / 2).min(box_h / 2);
        for py in 0..box_h {
            for px in 0..box_w {
                let cx = (ctx.x + px as i32) as u32;
                let cy = (ctx.y + py as i32) as u32;
                if cx >= cw || cy >= ch {
                    continue;
                }
                if is_outside_rounded_rect(px, py, box_w, box_h, radius) {
                    continue;
                }
                let bg = *ctx.buffer.get_pixel(cx, cy);
                ctx.buffer.put_pixel(cx, cy, blend(bg, self.background));
            }
        }

        let font_size = self.scale * 16.0;
        let total_line_height: f32 = measurements.iter().map(|(_, h)| h).sum();

        let (start_y, gap) = match self.align_y {
            Align::Top | Align::Left => (self.padding.1 as f32, spacing),
            Align::Center => (
                self.padding.1 as f32 + (inner_h as f32 - content_h) / 2.0,
                spacing,
            ),
            Align::Bottom | Align::Right => (
                self.padding.1 as f32 + (inner_h as f32 - content_h),
                spacing,
            ),
            Align::Spread => {
                let available = inner_h as f32 - total_line_height;
                let gap = if self.lines.len() > 1 {
                    available / (self.lines.len() - 1) as f32
                } else {
                    0.0
                };
                (self.padding.1 as f32, gap)
            }
        };

        let mut cursor_y = ctx.y as f32 + start_y;
        for (i, (line, (line_w, line_h))) in self.lines.iter().zip(measurements.iter()).enumerate()
        {
            let text_x = match self.align_x {
                Align::Left | Align::Top => ctx.x as f32 + self.padding.0 as f32,
                Align::Center => {
                    ctx.x as f32 + self.padding.0 as f32 + (inner_w as f32 - line_w) / 2.0
                }
                Align::Right | Align::Bottom => {
                    ctx.x as f32 + self.padding.0 as f32 + inner_w as f32 - line_w
                }
                Align::Spread => ctx.x as f32 + self.padding.0 as f32,
            };
            ctx.renderer.draw(
                ctx.buffer.as_mut(),
                cw,
                ch,
                text_x,
                cursor_y,
                line,
                font_size,
                self.shadow,
            );
            cursor_y += line_h;
            if i < self.lines.len() - 1 {
                cursor_y += gap;
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        let line_height = (self.scale * 16.0) as u32;
        let content_h = self.lines.len() as u32 * line_height
            + (self.lines.len().saturating_sub(1) as f32 * self.line_spacing * self.scale) as u32;
        (
            self.width.unwrap_or(100 + self.padding.0 * 2),
            self.height.unwrap_or(content_h + self.padding.1 * 2),
        )
    }
}
