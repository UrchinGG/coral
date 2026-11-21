use glam::{Mat4, Vec3};
use replay_model::{Equipment, Item, Material};
use replay_sim::{INVISIBLE, PLAYER_HEIGHT, PLAYER_WIDTH, PlayerFrame, Pose, SNEAK_DROP};

use crate::draw::Draw;
use crate::model::{ModelBox, Paint, cube};
use crate::{effects, items, text};

const HURT: u32 = 0xCC12128C;
const ON_FIRE: u8 = 0x01;
const NAME_HEIGHT: f32 = PLAYER_HEIGHT + 0.5;
const HEALTH_LIFT: f32 = 0.28;
const HEALTH_COLOR: u32 = 0xFF5555;
const ARMOR_INFLATE: f32 = 1.0;
const LEGGINGS_INFLATE: f32 = 0.5;

#[derive(Clone, Copy)]
enum Limb {
    Head,
    Body,
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Boots,
    Leggings,
    Chestplate,
    Helmet,
}

pub struct Person {
    pub name: String,
    pub color: u32,
    pub slim: bool,
    pub skin: i32,
}

const fn arm(u: f32, v: f32, x: f32, width: f32) -> ModelBox {
    cube(u, v, [x, -2.0, -2.0], [width, 12.0, 4.0])
}

const fn leg(u: f32, v: f32) -> ModelBox {
    cube(u, v, [-2.0, 0.0, -2.0], [4.0, 12.0, 4.0])
}

const HEAD: ModelBox = cube(0.0, 0.0, [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0]);
const HAT: ModelBox = cube(32.0, 0.0, [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0]).inflated(0.5);
const BODY: ModelBox = cube(16.0, 16.0, [-4.0, 0.0, -2.0], [8.0, 12.0, 4.0]);
const JACKET: ModelBox = cube(16.0, 32.0, [-4.0, 0.0, -2.0], [8.0, 12.0, 4.0]).inflated(0.25);

const fn skin(arm_width: f32, right_arm_x: f32) -> [(Limb, ModelBox); 12] {
    [
        (Limb::Head, HEAD),
        (Limb::Head, HAT),
        (Limb::Body, BODY),
        (Limb::Body, JACKET),
        (Limb::RightArm, arm(40.0, 16.0, right_arm_x, arm_width)),
        (
            Limb::RightArm,
            arm(40.0, 32.0, right_arm_x, arm_width).inflated(0.25),
        ),
        (Limb::LeftArm, arm(32.0, 48.0, -1.0, arm_width)),
        (
            Limb::LeftArm,
            arm(48.0, 48.0, -1.0, arm_width).inflated(0.25),
        ),
        (Limb::RightLeg, leg(0.0, 16.0)),
        (Limb::RightLeg, leg(0.0, 32.0).inflated(0.25)),
        (Limb::LeftLeg, leg(16.0, 48.0)),
        (Limb::LeftLeg, leg(0.0, 48.0).inflated(0.25)),
    ]
}

const CLASSIC: [(Limb, ModelBox); 12] = skin(4.0, -3.0);
const SLIM: [(Limb, ModelBox); 12] = skin(3.0, -2.0);

const HELMET: [(Limb, ModelBox); 2] = [(Limb::Head, HEAD), (Limb::Head, HAT)];
const CHESTPLATE: [(Limb, ModelBox); 3] = [
    (Limb::Body, BODY),
    (Limb::RightArm, arm(40.0, 16.0, -3.0, 4.0)),
    (Limb::LeftArm, arm(40.0, 16.0, -1.0, 4.0).mirrored()),
];
const LEGGINGS: [(Limb, ModelBox); 3] = [
    (Limb::Body, BODY),
    (Limb::RightLeg, leg(0.0, 16.0)),
    (Limb::LeftLeg, leg(0.0, 16.0).mirrored()),
];
const BOOTS: [(Limb, ModelBox); 2] = [
    (Limb::RightLeg, leg(0.0, 16.0)),
    (Limb::LeftLeg, leg(0.0, 16.0).mirrored()),
];

pub fn draw(draw: &mut Draw, frame: &PlayerFrame, person: &Person) {
    let pose = &frame.pose;
    let hurt = frame.hurt.is_some() || frame.death.is_some();
    let invisible = frame.state.flags & INVISIBLE != 0;
    if !invisible {
        let parts = if person.slim { &SLIM } else { &CLASSIC };
        let paint = Paint::skin(person.skin).flashed(if hurt { HURT } else { 0 });
        for &(limb, model) in parts {
            draw.cuboids(pose.root * limb.of(pose), paint, &[model]);
        }
    }
    wear(draw, pose, &frame.equipment, hurt);
    if frame.state.flags & ON_FIRE != 0 {
        effects::fire(draw, frame.position, PLAYER_WIDTH, PLAYER_HEIGHT);
    }
    if !invisible {
        let drop = if frame.sneaking() { SNEAK_DROP } else { 0.0 };
        let anchor = frame.position + Vec3::Y * (NAME_HEIGHT - drop);
        let name_anchor = match frame.state.health {
            Some(health) => {
                let label = format!("{} ❤", health.ceil() as i32);
                text::name_tag(draw, anchor, &label, HEALTH_COLOR, frame.sneaking());
                anchor + Vec3::Y * HEALTH_LIFT
            }
            None => anchor,
        };
        text::name_tag(
            draw,
            name_anchor,
            &person.name,
            person.color,
            frame.sneaking(),
        );
    }
}

pub fn wear(draw: &mut Draw, pose: &Pose, equipment: &Equipment, hurt: bool) {
    let slots = [
        (Slot::Boots, equipment.boots),
        (Slot::Leggings, equipment.leggings),
        (Slot::Chestplate, equipment.chestplate),
        (Slot::Helmet, equipment.helmet),
    ];
    for (slot, index) in slots {
        let Some((index, item)) = draw.item(index) else {
            continue;
        };
        match item.material {
            Some(material) => armor(draw, pose, slot, material, item, hurt),
            None if slot == Slot::Helmet => {
                items::on_head(draw, index, item, pose.root * pose.head)
            }
            None => {}
        }
    }
    if let Some((_, item)) = draw.item(equipment.held) {
        items::held(draw, item, pose.root * pose.held_item);
    }
}

fn armor(draw: &mut Draw, pose: &Pose, slot: Slot, material: Material, item: &Item, hurt: bool) {
    let legs = slot == Slot::Leggings;
    let (pieces, inflate): (&[(Limb, ModelBox)], f32) = match slot {
        Slot::Helmet => (&HELMET, ARMOR_INFLATE),
        Slot::Chestplate => (&CHESTPLATE, ARMOR_INFLATE),
        Slot::Leggings => (&LEGGINGS, LEGGINGS_INFLATE),
        Slot::Boots => (&BOOTS, ARMOR_INFLATE),
    };
    let flash = if hurt { HURT } else { 0 };
    let leather = material == Material::Leather;
    let dye = item
        .color
        .filter(|_| leather)
        .map_or(u32::MAX, |color| color << 8 | 0xFF);
    let sheet = |path| Paint::sheet(draw.assets.atlas.rect(path)).flashed(flash);
    let base = sheet(texture(material, legs))
        .tinted(dye)
        .glinting(item.enchanted);
    let overlay = leather.then(|| {
        sheet(if legs {
            "models/armor/leather_layer_2_overlay"
        } else {
            "models/armor/leather_layer_1_overlay"
        })
    });
    for paint in [Some(base), overlay].into_iter().flatten() {
        for &(limb, model) in pieces {
            draw.cuboids(pose.root * limb.of(pose), paint, &[model.inflated(inflate)]);
        }
    }
}

fn texture(material: Material, legs: bool) -> &'static str {
    match (material, legs) {
        (Material::Leather, false) => "models/armor/leather_layer_1",
        (Material::Leather, true) => "models/armor/leather_layer_2",
        (Material::Chainmail, false) => "models/armor/chainmail_layer_1",
        (Material::Chainmail, true) => "models/armor/chainmail_layer_2",
        (Material::Iron, false) => "models/armor/iron_layer_1",
        (Material::Iron, true) => "models/armor/iron_layer_2",
        (Material::Diamond, false) => "models/armor/diamond_layer_1",
        (Material::Diamond, true) => "models/armor/diamond_layer_2",
        (Material::Gold, false) => "models/armor/gold_layer_1",
        (Material::Gold, true) => "models/armor/gold_layer_2",
    }
}

impl Limb {
    fn of(self, pose: &Pose) -> Mat4 {
        match self {
            Limb::Head => pose.head,
            Limb::Body => pose.body,
            Limb::RightArm => pose.right_arm,
            Limb::LeftArm => pose.left_arm,
            Limb::RightLeg => pose.right_leg,
            Limb::LeftLeg => pose.left_leg,
        }
    }
}
