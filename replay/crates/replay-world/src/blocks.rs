use std::ops::{BitOr, Range};

#[derive(Clone, Debug, Default)]
pub struct BlockModels {
    pub blocks: Vec<Block>,
    pub forms: Vec<Form>,
    pub models: Vec<Range<usize>>,
    pub quads: Vec<Quad>,
    pub water: u16,
    pub lava: u16,
    pub item_frame: u16,
    pub destroy: [u16; 10],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Block {
    pub flags: Flags,
    pub rule: Rule,
    pub first_form: usize,
    pub particle: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Form {
    pub models: Range<usize>,
    pub tint: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub corners: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub layer: u16,
    pub face: Face,
    pub cull: Option<Face>,
    pub tinted: bool,
    pub shade: bool,
    pub smooth: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    East,
    West,
    Up,
    Down,
    South,
    North,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rule {
    #[default]
    Fixed,
    Snowy,
    DoublePlant,
    Tripwire,
    PumpkinStem,
    MelonStem,
    Vine,
    Fire,
    Redstone,
    Wall,
    Fence,
    Pane,
    Stairs,
    Door,
    Gate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(pub u16);

impl BlockModels {
    pub fn item(&self, state: u16) -> (&[Quad], u32) {
        self.forms
            .get(self.block(state).first_form)
            .map_or((&[], u32::MAX), |form| {
                (self.quads(form, [0; 3]), form.tint)
            })
    }

    pub fn block(&self, state: u16) -> Block {
        self.blocks
            .get(usize::from(state))
            .copied()
            .unwrap_or_default()
    }

    pub fn quads(&self, form: &Form, [x, y, z]: [i32; 3]) -> &[Quad] {
        let options = &self.models[form.models.clone()];
        if options.is_empty() {
            return &[];
        }
        let mut hash =
            (x as u32).wrapping_mul(3129871) ^ (z as u32).wrapping_mul(116129781) ^ y as u32;
        hash = (hash ^ hash >> 16).wrapping_mul(0x45d9f3b);
        hash ^= hash >> 16;
        &self.quads[options[hash as usize % options.len()].clone()]
    }
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::East,
        Face::West,
        Face::Up,
        Face::Down,
        Face::South,
        Face::North,
    ];

    pub(crate) fn axis(self) -> usize {
        self as usize / 2
    }

    pub(crate) fn positive(self) -> bool {
        (self as usize).is_multiple_of(2)
    }

    pub fn normal(self) -> [i32; 3] {
        let mut normal = [0; 3];
        normal[self.axis()] = if self.positive() { 1 } else { -1 };
        normal
    }

    pub fn shade(self) -> f32 {
        match self {
            Face::East | Face::West => 0.6,
            Face::Up => 1.0,
            Face::Down => 0.5,
            Face::South | Face::North => 0.8,
        }
    }
}

impl Rule {
    pub(crate) const ALL: [Rule; 15] = [
        Rule::Fixed,
        Rule::Snowy,
        Rule::DoublePlant,
        Rule::Tripwire,
        Rule::PumpkinStem,
        Rule::MelonStem,
        Rule::Vine,
        Rule::Fire,
        Rule::Redstone,
        Rule::Wall,
        Rule::Fence,
        Rule::Pane,
        Rule::Stairs,
        Rule::Door,
        Rule::Gate,
    ];

    pub fn forms(self) -> usize {
        match self {
            Rule::Fixed => 1,
            Rule::Snowy | Rule::Vine | Rule::Gate => 2,
            Rule::PumpkinStem | Rule::MelonStem | Rule::Stairs => 5,
            Rule::DoublePlant | Rule::Door => 8,
            Rule::Tripwire | Rule::Fence | Rule::Pane => 16,
            Rule::Wall => 32,
            Rule::Redstone => 81,
            Rule::Fire => 192,
        }
    }
}

impl Flags {
    pub const OCCLUDES: Flags = Flags(1);
    pub const SOLID: Flags = Flags(1 << 1);
    pub const TRANSLUCENT: Flags = Flags(1 << 2);
    pub const SELF_CULL: Flags = Flags(1 << 3);
    pub const WATER: Flags = Flags(1 << 4);
    pub const LAVA: Flags = Flags(1 << 5);
    pub const FLAMMABLE: Flags = Flags(1 << 6);
    pub const GLASS: Flags = Flags(1 << 7);
    pub const GOURD: Flags = Flags(1 << 8);
    pub const PUMPKIN: Flags = Flags(1 << 9);
    pub const MELON: Flags = Flags(1 << 10);
    pub const NETHER_FENCE: Flags = Flags(1 << 11);
    pub const REDSTONE: Flags = Flags(1 << 12);
    pub const SNOW: Flags = Flags(1 << 13);
    pub const TRIPWIRE: Flags = Flags(1 << 14);

    pub fn has(self, flags: Flags) -> bool {
        self.0 & flags.0 != 0
    }
}

impl BitOr for Flags {
    type Output = Flags;

    fn bitor(self, other: Flags) -> Flags {
        Flags(self.0 | other.0)
    }
}
