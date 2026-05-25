use crate::canvas::{context::DrawContext, shape::Shape};

pub struct Column {
    children: Vec<Box<dyn Shape>>,
    gap: u32,
}

impl Column {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: 0,
        }
    }

    pub fn gap(mut self, gap: u32) -> Self {
        self.gap = gap;
        self
    }

    pub fn push<S: Shape + 'static>(mut self, shape: S) -> Self {
        self.children.push(Box::new(shape));
        self
    }
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape for Column {
    fn draw(&self, ctx: &mut DrawContext) {
        let mut cursor_y = 0i32;
        for child in &self.children {
            child.draw(&mut ctx.at(0, cursor_y));
            cursor_y += child.size().1 as i32 + self.gap as i32;
        }
    }

    fn size(&self) -> (u32, u32) {
        let mut max_w = 0u32;
        let mut total_h = 0u32;
        for (i, child) in self.children.iter().enumerate() {
            let (w, h) = child.size();
            max_w = max_w.max(w);
            total_h += h;
            if i > 0 {
                total_h += self.gap;
            }
        }
        (max_w, total_h)
    }
}
