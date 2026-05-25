use mctext::MCText;

use crate::canvas::{context::DrawContext, shape::Shape, text::TextRenderer};

use super::text_box::Align;

pub struct Text {
    text: MCText,
    scale: f32,
    align_x: Align,
    shadow: bool,
    max_width: Option<u32>,
}

impl Text {
    pub fn new(text: MCText) -> Self {
        Self {
            text,
            scale: 1.0,
            align_x: Align::Left,
            shadow: true,
            max_width: None,
        }
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn align_x(mut self, align: Align) -> Self {
        self.align_x = align;
        self
    }

    pub fn shadow(mut self, shadow: bool) -> Self {
        self.shadow = shadow;
        self
    }

    pub fn max_width(mut self, width: u32) -> Self {
        self.max_width = Some(width);
        self
    }
}

impl Shape for Text {
    fn measure(&self, renderer: &TextRenderer) -> (u32, u32) {
        let font_size = self.scale * 16.0;
        let (w, h) = renderer.measure(&self.text, font_size);
        let effective_w = self
            .max_width
            .map(|max| (w as u32).min(max))
            .unwrap_or(w.ceil() as u32);
        (effective_w, h.ceil() as u32)
    }

    fn draw(&self, ctx: &mut DrawContext) {
        let (cw, ch) = ctx.buffer.dimensions();
        let font_size = self.scale * 16.0;
        let (text_w, _) = ctx.renderer.measure(&self.text, font_size);

        let effective_scale = self
            .max_width
            .filter(|&max_w| text_w > max_w as f32)
            .map(|max_w| self.scale * (max_w as f32 / text_w))
            .unwrap_or(self.scale);
        let effective_font_size = effective_scale * 16.0;
        let final_w = if effective_scale != self.scale {
            ctx.renderer.measure(&self.text, effective_font_size).0
        } else {
            text_w
        };

        let text_x = match self.align_x {
            Align::Left | Align::Top | Align::Spread => ctx.x as f32,
            Align::Center => ctx.x as f32 + (self.max_width.unwrap_or(0) as f32 - final_w) / 2.0,
            Align::Right | Align::Bottom => {
                ctx.x as f32 + self.max_width.unwrap_or(0) as f32 - final_w
            }
        };

        ctx.renderer.draw(
            ctx.buffer.as_mut(),
            cw,
            ch,
            text_x,
            ctx.y as f32,
            &self.text,
            effective_font_size,
            self.shadow,
        );
    }

    fn size(&self) -> (u32, u32) {
        (0, 0)
    }
}

pub struct TextBlock {
    lines: Vec<MCText>,
    scale: f32,
    line_spacing: f32,
    align_x: Align,
    shadow: bool,
    max_width: Option<u32>,
}

impl TextBlock {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            scale: 1.0,
            line_spacing: 8.0,
            align_x: Align::Left,
            shadow: true,
            max_width: None,
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

    pub fn shadow(mut self, shadow: bool) -> Self {
        self.shadow = shadow;
        self
    }

    pub fn max_width(mut self, width: u32) -> Self {
        self.max_width = Some(width);
        self
    }
}

impl Default for TextBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape for TextBlock {
    fn measure(&self, renderer: &TextRenderer) -> (u32, u32) {
        let font_size = self.scale * 16.0;
        let mut max_width = 0.0f32;
        let mut total_height = 0.0f32;
        for (i, line) in self.lines.iter().enumerate() {
            let (w, h) = renderer.measure(line, font_size);
            max_width = max_width.max(w);
            total_height += h;
            if i < self.lines.len() - 1 {
                total_height += self.line_spacing * self.scale;
            }
        }
        let effective_w = self
            .max_width
            .map(|m| (max_width as u32).min(m))
            .unwrap_or(max_width.ceil() as u32);
        (effective_w, total_height.ceil() as u32)
    }

    fn draw(&self, ctx: &mut DrawContext) {
        let (cw, ch) = ctx.buffer.dimensions();
        let font_size = self.scale * 16.0;
        let mut cursor_y = ctx.y as f32;

        for line in &self.lines {
            let (text_w, text_h) = ctx.renderer.measure(line, font_size);
            let effective_scale = self
                .max_width
                .filter(|&max_w| text_w > max_w as f32)
                .map(|max_w| self.scale * (max_w as f32 / text_w))
                .unwrap_or(self.scale);
            let effective_font_size = effective_scale * 16.0;
            let (final_w, final_h) = if effective_scale != self.scale {
                ctx.renderer.measure(line, effective_font_size)
            } else {
                (text_w, text_h)
            };

            let text_x = match self.align_x {
                Align::Left | Align::Top | Align::Spread => ctx.x as f32,
                Align::Center => {
                    ctx.x as f32 + (self.max_width.unwrap_or(0) as f32 - final_w) / 2.0
                }
                Align::Right | Align::Bottom => {
                    ctx.x as f32 + self.max_width.unwrap_or(0) as f32 - final_w
                }
            };

            ctx.renderer.draw(
                ctx.buffer.as_mut(),
                cw,
                ch,
                text_x,
                cursor_y,
                line,
                effective_font_size,
                self.shadow,
            );
            cursor_y += final_h + (self.line_spacing * effective_scale);
        }
    }

    fn size(&self) -> (u32, u32) {
        (0, 0)
    }
}
