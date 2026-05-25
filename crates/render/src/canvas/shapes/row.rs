use crate::canvas::{context::DrawContext, shape::Shape};

pub struct Row {
    children: Vec<Box<dyn Shape>>,
    gap: u32,
}

impl Row {
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

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape for Row {
    fn draw(&self, ctx: &mut DrawContext) {
        let mut cursor_x = 0i32;
        for child in &self.children {
            child.draw(&mut ctx.at(cursor_x, 0));
            cursor_x += child.size().0 as i32 + self.gap as i32;
        }
    }

    fn size(&self) -> (u32, u32) {
        let mut total_w = 0u32;
        let mut max_h = 0u32;
        for (i, child) in self.children.iter().enumerate() {
            let (w, h) = child.size();
            total_w += w;
            if i > 0 {
                total_w += self.gap;
            }
            max_h = max_h.max(h);
        }
        (total_w, max_h)
    }
}
