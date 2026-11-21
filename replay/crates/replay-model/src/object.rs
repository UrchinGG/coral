use replay_protocol::{Metadata, MetadataValue, Nbt, Slot};

use crate::item::{Items, known_block};

const MIN_ARMOR_STAND_MS: u32 = 150;
const VELOCITY_UNIT: f32 = 1.0 / 8000.0;
const MAX_EXPLOSIONS: usize = 8;
const MAX_COLORS: usize = 8;

#[derive(Debug, Default)]
pub struct Object {
    pub kind: ObjectKind,
    pub path: Path,
    pub end: Option<u32>,
    pub velocity: Vec<(u32, [f32; 3])>,
    pub names: Vec<(u32, Option<String>)>,
    pub riding: Vec<(u32, Option<usize>)>,
    pub attacks: Vec<u32>,
    pub burning: Vec<(u32, bool)>,
    pub burst: Option<u32>,
    pub item: Option<u32>,
    pub block: Option<u16>,
    pub facing: u8,
    pub rotation: u8,
    pub explosions: Vec<FireworkExplosion>,
    pub profession: i32,
    pub helmet: Option<u32>,
    pub hand: Option<u32>,
    pub invisible: bool,
    pub small: bool,
    pub marker: bool,
    pub critical: bool,
}

#[derive(Debug, Default)]
pub struct Path {
    pub t: Vec<u32>,
    pub pos: Vec<[f32; 3]>,
    pub look: Vec<Look>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireworkExplosion {
    pub shape: FireworkShape,
    pub colors: Vec<u32>,
    pub fade: Vec<u32>,
    pub flicker: bool,
    pub trail: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireworkShape {
    SmallBall,
    LargeBall,
    Star,
    Creeper,
    Burst,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    #[default]
    Item,
    Tnt,
    Arrow,
    Snowball,
    Egg,
    Fireball,
    SmallFireball,
    EnderPearl,
    FallingBlock,
    ItemFrame,
    Potion,
    Firework,
    ArmorStand,
    Silverfish,
    EnderDragon,
    Bat,
    IronGolem,
    Villager,
}

impl ObjectKind {
    pub(crate) fn object(kind: u8) -> Option<ObjectKind> {
        Some(match kind {
            2 => ObjectKind::Item,
            50 => ObjectKind::Tnt,
            60 => ObjectKind::Arrow,
            61 => ObjectKind::Snowball,
            62 => ObjectKind::Egg,
            63 => ObjectKind::Fireball,
            64 => ObjectKind::SmallFireball,
            65 => ObjectKind::EnderPearl,
            70 => ObjectKind::FallingBlock,
            71 => ObjectKind::ItemFrame,
            73 => ObjectKind::Potion,
            76 => ObjectKind::Firework,
            78 => ObjectKind::ArmorStand,
            _ => return None,
        })
    }

    pub(crate) fn mob(kind: u8) -> Option<ObjectKind> {
        Some(match kind {
            60 => ObjectKind::Silverfish,
            63 => ObjectKind::EnderDragon,
            65 => ObjectKind::Bat,
            99 => ObjectKind::IronGolem,
            120 => ObjectKind::Villager,
            _ => return None,
        })
    }

    pub fn height(self) -> f32 {
        match self {
            ObjectKind::Item
            | ObjectKind::Snowball
            | ObjectKind::Egg
            | ObjectKind::EnderPearl
            | ObjectKind::Potion
            | ObjectKind::Firework => 0.25,
            ObjectKind::Tnt | ObjectKind::FallingBlock => 0.98,
            ObjectKind::Arrow | ObjectKind::ItemFrame => 0.5,
            ObjectKind::Fireball => 1.0,
            ObjectKind::SmallFireball => 0.3125,
            ObjectKind::ArmorStand => 1.975,
            ObjectKind::Silverfish => 0.3,
            ObjectKind::EnderDragon => 8.0,
            ObjectKind::Bat => 0.9,
            ObjectKind::IronGolem => 2.9,
            ObjectKind::Villager => 1.8,
        }
    }

    pub fn width(self) -> f32 {
        match self {
            ObjectKind::ArmorStand | ObjectKind::Bat => 0.5,
            ObjectKind::Silverfish => 0.4,
            ObjectKind::EnderDragon => 16.0,
            ObjectKind::IronGolem => 1.4,
            ObjectKind::Villager => 0.6,
            kind => kind.height(),
        }
    }

    fn turns(self) -> bool {
        matches!(
            self,
            ObjectKind::Arrow
                | ObjectKind::ArmorStand
                | ObjectKind::Silverfish
                | ObjectKind::EnderDragon
                | ObjectKind::Bat
                | ObjectKind::IronGolem
                | ObjectKind::Villager
        )
    }

    fn moves(self) -> bool {
        matches!(
            self,
            ObjectKind::Item
                | ObjectKind::Tnt
                | ObjectKind::Arrow
                | ObjectKind::Snowball
                | ObjectKind::Egg
                | ObjectKind::EnderPearl
                | ObjectKind::FallingBlock
                | ObjectKind::Potion
                | ObjectKind::Firework
        )
    }

    fn named(self) -> bool {
        matches!(
            self,
            ObjectKind::ArmorStand
                | ObjectKind::Silverfish
                | ObjectKind::IronGolem
                | ObjectKind::Villager
        )
    }
}

impl Object {
    pub(crate) fn spawn(kind: ObjectKind, data: i32) -> Object {
        let mut object = Object {
            kind,
            ..Object::default()
        };
        match kind {
            ObjectKind::FallingBlock => {
                let id = data & 0xFFF;
                object.block = known_block(id).then_some((id << 4 | (data >> 12 & 15)) as u16);
            }
            ObjectKind::ItemFrame => object.facing = (data & 3) as u8,
            _ => {}
        }
        object
    }

    pub(crate) fn apply(&mut self, metadata: &Metadata, items: &mut Items) {
        for (index, value) in metadata {
            match (self.kind, index, value) {
                (_, 0, MetadataValue::Byte(flags)) => self.invisible = flags & 0x20 != 0,
                (ObjectKind::Item, 10, MetadataValue::Slot(slot)) => {
                    self.item = items.intern(slot.as_ref()).or(self.item)
                }
                (ObjectKind::ItemFrame, 8, MetadataValue::Slot(slot)) => {
                    self.item = items.intern(slot.as_ref())
                }
                (ObjectKind::ItemFrame, 9, MetadataValue::Byte(rotation)) => {
                    self.rotation = rotation & 7
                }
                (ObjectKind::Firework, 8, MetadataValue::Slot(slot)) => {
                    self.explosions = explosions(slot.as_ref())
                }
                (ObjectKind::ArmorStand, 10, MetadataValue::Byte(flags)) => {
                    self.small = flags & 1 != 0;
                    self.marker = flags & 16 != 0;
                }
                (ObjectKind::Arrow, 16, MetadataValue::Byte(flags)) => {
                    self.critical = flags & 1 != 0
                }
                (ObjectKind::Villager, 16, MetadataValue::Int(profession)) => {
                    self.profession = *profession
                }
                _ => {}
            }
        }
    }

    pub(crate) fn record_path(&mut self, t: u32, pos: [f32; 3], look: Look) {
        let look = self.kind.turns().then_some(look);
        if self.path.pos.last() == Some(&pos) && self.path.look.last() == look.as_ref() {
            return;
        }
        self.path.t.push(t);
        self.path.pos.push(pos);
        self.path.look.extend(look);
    }

    pub(crate) fn record_name(&mut self, t: u32, name: Option<String>) {
        let last = self.names.last().and_then(|(_, name)| name.as_ref());
        if self.kind.named() && last != name.as_ref() {
            self.names.push((t, name));
        }
    }

    pub(crate) fn record_burning(&mut self, t: u32, burning: bool) {
        if self.burning.last().is_some_and(|&(_, was)| was) != burning {
            self.burning.push((t, burning));
        }
    }

    pub(crate) fn record_velocity(&mut self, t: u32, velocity: [i16; 3]) {
        if self.kind.moves() {
            self.velocity
                .push((t, velocity.map(|axis| f32::from(axis) * VELOCITY_UNIT)));
        }
    }

    pub(crate) fn shown(&self) -> bool {
        let Some(&start) = self.path.t.first() else {
            return false;
        };
        let lasted = self.end.is_none_or(|end| end - start >= MIN_ARMOR_STAND_MS);
        let shows = self.helmet.is_some()
            || self.hand.is_some()
            || self.names.iter().any(|(_, name)| name.is_some());
        self.kind != ObjectKind::ArmorStand || (lasted && shows)
    }
}

fn explosions(slot: Option<&Slot>) -> Vec<FireworkExplosion> {
    let explosions = slot.and_then(|slot| slot.nbt.as_ref()?.get("Fireworks")?.get("Explosions"));
    let Some(Nbt::List(explosions)) = explosions else {
        return Vec::new();
    };
    explosions
        .iter()
        .take(MAX_EXPLOSIONS)
        .map(FireworkExplosion::new)
        .collect()
}

impl FireworkExplosion {
    fn new(nbt: &Nbt) -> FireworkExplosion {
        let flag = |key| matches!(nbt.get(key), Some(Nbt::Byte(value)) if *value != 0);
        let colors = |key| match nbt.get(key) {
            Some(Nbt::IntArray(colors)) => colors
                .iter()
                .take(MAX_COLORS)
                .map(|&color| color as u32 & 0xFFFFFF)
                .collect(),
            _ => Vec::new(),
        };
        FireworkExplosion {
            shape: match nbt.get("Type") {
                Some(Nbt::Byte(1)) => FireworkShape::LargeBall,
                Some(Nbt::Byte(2)) => FireworkShape::Star,
                Some(Nbt::Byte(3)) => FireworkShape::Creeper,
                Some(Nbt::Byte(4)) => FireworkShape::Burst,
                _ => FireworkShape::SmallBall,
            },
            colors: colors("Colors"),
            fade: colors("FadeColors"),
            flicker: flag("Flicker"),
            trail: flag("Trail"),
        }
    }
}
