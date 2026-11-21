use std::fmt::Display;

use replay_world::{Flags, Rule};

use super::{BlockInfo, model::Pick};

pub const COLORS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "silver",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];
const WOODS: [&str; 6] = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak"];
const HORIZONTAL: [&str; 4] = ["south", "west", "north", "east"];
const FRONT: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const SIDES: [&str; 4] = ["north", "east", "south", "west"];
const AXES: [&str; 4] = ["y", "x", "z", "none"];
const RAILS: [&str; 10] = [
    "north_south",
    "east_west",
    "ascending_east",
    "ascending_west",
    "ascending_north",
    "ascending_south",
    "south_east",
    "south_west",
    "north_west",
    "north_east",
];
const DOUBLE_PLANTS: [&str; 6] = ["sunflower", "syringa", "grass", "fern", "rose", "paeonia"];
const STAIR_SHAPES: [&str; 5] = [
    "straight",
    "inner_left",
    "inner_right",
    "outer_left",
    "outer_right",
];
const DRAWN_AS_NOTHING: [&str; 14] = [
    "air",
    "water",
    "flowing_water",
    "lava",
    "flowing_lava",
    "standing_banner",
    "wall_banner",
    "skull",
    "end_portal",
    "piston_extension",
    "barrier",
    "chest",
    "trapped_chest",
    "ender_chest",
];
const OPAQUE_CUBES: [&str; 16] = [
    "tnt",
    "furnace",
    "lit_furnace",
    "redstone_ore",
    "lit_redstone_ore",
    "pumpkin",
    "lit_pumpkin",
    "glowstone",
    "melon_block",
    "redstone_lamp",
    "lit_redstone_lamp",
    "double_stone_slab",
    "double_wooden_slab",
    "double_stone_slab2",
    "redstone_block",
    "sea_lantern",
];
const FLAMMABLE: [&str; 36] = [
    "planks",
    "double_wooden_slab",
    "wooden_slab",
    "fence",
    "spruce_fence",
    "birch_fence",
    "jungle_fence",
    "dark_oak_fence",
    "acacia_fence",
    "fence_gate",
    "spruce_fence_gate",
    "birch_fence_gate",
    "jungle_fence_gate",
    "dark_oak_fence_gate",
    "acacia_fence_gate",
    "oak_stairs",
    "spruce_stairs",
    "birch_stairs",
    "jungle_stairs",
    "acacia_stairs",
    "dark_oak_stairs",
    "log",
    "log2",
    "leaves",
    "leaves2",
    "bookshelf",
    "tnt",
    "tallgrass",
    "double_plant",
    "yellow_flower",
    "red_flower",
    "deadbush",
    "wool",
    "vine",
    "coal_block",
    "hay_block",
];
const REDSTONE: [&str; 20] = [
    "redstone_wire",
    "redstone_torch",
    "unlit_redstone_torch",
    "redstone_block",
    "lever",
    "stone_button",
    "wooden_button",
    "stone_pressure_plate",
    "wooden_pressure_plate",
    "light_weighted_pressure_plate",
    "heavy_weighted_pressure_plate",
    "detector_rail",
    "tripwire_hook",
    "daylight_detector",
    "daylight_detector_inverted",
    "unpowered_repeater",
    "powered_repeater",
    "unpowered_comparator",
    "powered_comparator",
    "trapped_chest",
];
const GLASS: [&str; 5] = [
    "glass",
    "stained_glass",
    "glass_pane",
    "stained_glass_pane",
    "iron_bars",
];
const TRANSLUCENT: [&str; 7] = [
    "water",
    "flowing_water",
    "ice",
    "stained_glass",
    "stained_glass_pane",
    "slime",
    "portal",
];
const SELF_CULL: [&str; 8] = [
    "glass",
    "stained_glass",
    "glass_pane",
    "stained_glass_pane",
    "iron_bars",
    "ice",
    "slime",
    "portal",
];

pub enum Variant {
    Nothing,
    Model(Pick),
    State {
        key: String,
        rename: Option<(&'static str, String)>,
    },
}

pub fn rule(name: &str, meta: u16) -> Rule {
    match name {
        "grass" | "mycelium" => Rule::Snowy,
        "double_plant" if meta & 8 != 0 => Rule::DoublePlant,
        "tripwire" => Rule::Tripwire,
        "pumpkin_stem" => Rule::PumpkinStem,
        "melon_stem" => Rule::MelonStem,
        "vine" => Rule::Vine,
        "fire" => Rule::Fire,
        "redstone_wire" => Rule::Redstone,
        "cobblestone_wall" => Rule::Wall,
        "glass_pane" | "iron_bars" | "stained_glass_pane" => Rule::Pane,
        _ if name.ends_with("fence") => Rule::Fence,
        _ if name.ends_with("fence_gate") => Rule::Gate,
        _ if name.ends_with("_stairs") => Rule::Stairs,
        _ if name.ends_with("_door") => Rule::Door,
        _ => Rule::Fixed,
    }
}

pub fn flags(block: &BlockInfo, meta: u16) -> Flags {
    let name = block.name.as_str();
    let visible = !matches!(name, "air" | "barrier");
    let occludes = visible
        && match name {
            "piston" | "sticky_piston" => meta & 8 == 0,
            "piston_head" => false,
            _ => {
                OPAQUE_CUBES.contains(&name) || !block.transparent && block.bounding_box == "block"
            }
        };
    [
        (occludes, Flags::OCCLUDES),
        (name != "air" && block.bounding_box != "empty", Flags::SOLID),
        (TRANSLUCENT.contains(&name), Flags::TRANSLUCENT),
        (SELF_CULL.contains(&name), Flags::SELF_CULL),
        (matches!(name, "water" | "flowing_water"), Flags::WATER),
        (matches!(name, "lava" | "flowing_lava"), Flags::LAVA),
        (FLAMMABLE.contains(&name), Flags::FLAMMABLE),
        (GLASS.contains(&name), Flags::GLASS),
        (
            matches!(name, "pumpkin" | "lit_pumpkin" | "melon_block"),
            Flags::GOURD,
        ),
        (name == "pumpkin", Flags::PUMPKIN),
        (name == "melon_block", Flags::MELON),
        (name == "nether_brick_fence", Flags::NETHER_FENCE),
        (REDSTONE.contains(&name), Flags::REDSTONE),
        (matches!(name, "snow" | "snow_layer"), Flags::SNOW),
        (
            matches!(name, "tripwire" | "tripwire_hook"),
            Flags::TRIPWIRE,
        ),
    ]
    .into_iter()
    .filter(|&(on, _)| on)
    .fold(Flags::default(), |flags, (_, flag)| flags | flag)
}

pub fn tint(name: &str, meta: u16, form: usize, grass: u32, foliage: u32) -> u32 {
    let rgb = match name {
        "grass" | "reeds" => grass,
        "tallgrass" if meta != 0 => grass,
        "double_plant" => {
            let kind = if meta & 8 != 0 {
                form
            } else {
                usize::from(meta & 7)
            };
            if matches!(kind, 0 | 1 | 4 | 5) {
                0xffffff
            } else {
                grass
            }
        }
        "vine" | "leaves2" => foliage,
        "leaves" => [foliage, 0x619961, 0x80a755, foliage][usize::from(meta & 3)],
        "waterlily" => 0x208030,
        "pumpkin_stem" | "melon_stem" => {
            let age = u32::from(meta & 7);
            (age * 32) << 16 | (255 - age * 8) << 8 | (age * 4)
        }
        "redstone_wire" => {
            let level = f32::from(meta) / 15.0;
            let red = level * 0.6 + if level > 0.0 { 0.4 } else { 0.3 };
            let green = (level * level * 0.7 - 0.5).max(0.0);
            let blue = (level * level * 0.6 - 0.7).max(0.0);
            [red, green, blue].into_iter().fold(0, |rgb, channel| {
                rgb << 8 | (channel * 255.0).round() as u32
            })
        }
        _ => 0xffffff,
    };
    rgb << 8 | 0xff
}

pub fn variant(name: &str, meta: u16, form: usize) -> Variant {
    let index = usize::from(meta);
    let on = |bit: u16| meta & bit != 0;
    let bit = |bit: usize| form >> bit & 1 == 1;
    let color = COLORS[index];
    let wood = |index: usize| WOODS.get(index).copied().unwrap_or("oak");
    let half = if on(8) { "upper" } else { "half" };
    let links = || {
        SIDES
            .iter()
            .zip(0..)
            .map(|(side, index)| prop(side, bit(index)))
    };
    let facing = HORIZONTAL[index & 3];
    let front = FRONT.get(index & 7).copied();
    match name {
        _ if DRAWN_AS_NOTHING.contains(&name) => Variant::Nothing,
        "standing_sign" => rotated("viewer_sign_standing", 0.0, f32::from(meta) * 22.5),
        "wall_sign" => {
            let turn = match front {
                Some("east") => 90.0,
                Some("south") => 180.0,
                Some("west") => 270.0,
                _ => 0.0,
            };
            rotated("viewer_sign_wall", 0.0, turn)
        }
        "stone" if meta != 0 => model(pick(
            &[
                "stone",
                "granite",
                "granite_smooth",
                "diorite",
                "diorite_smooth",
                "andesite",
                "andesite_smooth",
            ],
            index,
            "stone",
        )),
        "dirt" if meta != 0 => model(pick(&["dirt", "coarse_dirt", "podzol"], index, "dirt")),
        "planks" | "double_wooden_slab" => model(format!("{}_planks", wood(index & 7))),
        "sand" if meta == 1 => model("red_sand"),
        "sandstone" => model(pick(
            &["sandstone_normal", "sandstone_chiseled", "sandstone_smooth"],
            index,
            "sandstone_normal",
        )),
        "red_sandstone" => model(pick(
            &[
                "red_sandstone_normal",
                "red_sandstone_chiseled",
                "red_sandstone_smooth",
            ],
            index,
            "red_sandstone_normal",
        )),
        "wool" => model(format!("{color}_wool")),
        "carpet" => model(format!("carpet_{color}")),
        "stained_hardened_clay" => model(format!("hardened_clay_{color}")),
        "stained_glass" => Variant::Model(Pick {
            model: "glass_white".into(),
            retexture: Some(("glass_white".into(), format!("glass_{color}"))),
            ..Pick::default()
        }),
        "stonebrick" => model(pick(
            &[
                "stonebrick_normal",
                "stonebrick_mossy",
                "stonebrick_cracked",
                "stonebrick_chiseled",
            ],
            index,
            "stonebrick_normal",
        )),
        "monster_egg" => model(pick(
            &[
                "stone",
                "cobblestone",
                "stonebrick_normal",
                "stonebrick_mossy",
                "stonebrick_cracked",
                "stonebrick_chiseled",
            ],
            index,
            "stone",
        )),
        "prismarine" => model(pick(
            &["prismarine_rough", "prismarine_bricks", "prismarine_dark"],
            index,
            "prismarine_rough",
        )),
        "sponge" => state([prop("wet", meta == 1)]),
        "sapling" => state([prop("stage", u8::from(on(8)))]).renamed("oak_", wood(index & 7)),
        "log" => state([prop("axis", AXES[index >> 2])]).renamed("oak_", WOODS[index & 3]),
        "log2" => state([prop("axis", AXES[index >> 2])]).renamed("acacia_", wood(4 + (index & 1))),
        "leaves" => model(format!("{}_leaves", WOODS[index & 3])),
        "leaves2" => model(format!("{}_leaves", wood(4 + (index & 1)))),
        "tallgrass" => model(pick(
            &["dead_bush", "tall_grass", "fern"],
            index,
            "tall_grass",
        )),
        "yellow_flower" => model("dandelion"),
        "red_flower" => model(pick(
            &[
                "poppy",
                "orchid",
                "allium",
                "houstonia",
                "red_tulip",
                "orange_tulip",
                "white_tulip",
                "pink_tulip",
                "daisy",
            ],
            index,
            "poppy",
        )),
        "double_plant" => {
            let kind = if on(8) { form } else { index & 7 };
            let part = if on(8) { "top" } else { "bottom" };
            model(format!(
                "double_{}_{part}",
                pick(&DOUBLE_PLANTS, kind, "grass")
            ))
        }
        "quartz_block" => match meta {
            1 => model("quartz_chiseled"),
            2 => model("quartz_lines"),
            3 => rotated("quartz_lines", 90.0, 90.0),
            4 => rotated("quartz_lines", 90.0, 0.0),
            _ => model("quartz_normal"),
        },
        "hay_block" => state([prop(
            "axis",
            match meta & 12 {
                4 => "x",
                8 => "z",
                _ => "y",
            },
        )]),
        "double_stone_slab" => model(match (meta & 7, on(8)) {
            (0, true) => "double_stone_top",
            (0, false) => "double_stone",
            (1, true) => "sandstone_all",
            (1, false) => "sandstone_normal",
            (2, _) => "oak_planks",
            (3, _) => "cobblestone",
            (4, _) => "brick",
            (5, _) => "stonebrick_normal",
            (6, _) => "nether_brick",
            (_, true) => "quartz_top",
            (_, false) => "quartz_normal",
        }),
        "stone_slab" => {
            let kind = [
                "stone",
                "sandstone",
                "oak",
                "cobblestone",
                "brick",
                "stone_brick",
                "nether_brick",
                "quartz",
            ];
            model(format!("{half}_slab_{}", kind[index & 7]))
        }
        "stone_slab2" => model(format!("{half}_slab_red_sandstone")),
        "double_stone_slab2" => model(if on(8) {
            "red_sandstone_all"
        } else {
            "red_sandstone_normal"
        }),
        "wooden_slab" => model(format!("{half}_slab_{}", wood(index & 7))),
        "grass" | "mycelium" => state([prop("snowy", bit(0))]),
        "torch" | "redstone_torch" | "unlit_redstone_torch" => state([prop(
            "facing",
            pick(&["up", "east", "west", "south", "north", "up"], index, "up"),
        )]),
        "ladder" | "furnace" | "lit_furnace" => state([prop(
            "facing",
            front
                .filter(|&side| side != "up" && side != "down")
                .unwrap_or("north"),
        )]),
        "pumpkin" | "lit_pumpkin" => state([prop("facing", facing)]),
        "dispenser" | "dropper" => state([prop("facing", front.unwrap_or("north"))]),
        "hopper" => state([prop(
            "facing",
            front.filter(|&side| side != "up").unwrap_or("down"),
        )]),
        "piston" | "sticky_piston" => state([
            prop("extended", on(8)),
            prop("facing", front.unwrap_or("up")),
        ]),
        "piston_head" => state([
            prop("facing", front.unwrap_or("up")),
            prop("short", false),
            prop("type", if on(8) { "sticky" } else { "normal" }),
        ]),
        "bed" => state([
            prop("facing", facing),
            prop("part", if on(8) { "head" } else { "foot" }),
        ]),
        "rail" => state([prop("shape", pick(&RAILS, index, "north_south"))]),
        "golden_rail" | "detector_rail" | "activator_rail" => state([
            prop("powered", on(8)),
            prop("shape", pick(&RAILS[..6], index & 7, "north_south")),
        ]),
        "lever" => {
            let lever = [
                "down_x", "east", "west", "south", "north", "up_z", "up_x", "down_z",
            ];
            state([prop("facing", lever[index & 7]), prop("powered", on(8))])
        }
        "stone_button" | "wooden_button" => state([
            prop(
                "facing",
                pick(
                    &["down", "east", "west", "south", "north", "up"],
                    index & 7,
                    "up",
                ),
            ),
            prop("powered", on(8)),
        ]),
        "stone_pressure_plate" | "wooden_pressure_plate" => state([prop("powered", meta == 1)]),
        "light_weighted_pressure_plate"
        | "heavy_weighted_pressure_plate"
        | "daylight_detector"
        | "daylight_detector_inverted" => state([prop("power", meta)]),
        "snow_layer" => state([prop("layers", (meta & 7) + 1)]),
        "farmland" => state([prop("moisture", meta & 7)]),
        "wheat" | "carrots" | "potatoes" => state([prop("age", meta & 7)]),
        "nether_wart" => state([prop("age", meta & 3)]),
        "cake" => state([prop("bites", meta.min(6))]),
        "cauldron" => state([prop("level", meta & 3)]),
        "brewing_stand" => state([
            prop("has_bottle_0", on(1)),
            prop("has_bottle_1", on(2)),
            prop("has_bottle_2", on(4)),
        ]),
        "end_portal_frame" => state([prop("eye", on(4)), prop("facing", facing)]),
        "cocoa" => state([prop("age", (meta >> 2).min(2)), prop("facing", facing)]),
        "anvil" => state([prop("damage", (meta >> 2).min(2)), prop("facing", facing)]),
        "portal" => state([prop("axis", if meta & 3 == 2 { "z" } else { "x" })]),
        "unpowered_repeater" | "powered_repeater" => state([
            prop("delay", (meta >> 2) + 1),
            prop("facing", facing),
            prop("locked", false),
        ]),
        "unpowered_comparator" | "powered_comparator" => state([
            prop("facing", facing),
            prop("mode", if on(4) { "subtract" } else { "compare" }),
            prop("powered", on(8)),
        ]),
        "trapdoor" | "iron_trapdoor" => state([
            prop("facing", ["north", "south", "west", "east"][index & 3]),
            prop("half", if on(8) { "top" } else { "bottom" }),
            prop("open", on(4)),
        ]),
        "brown_mushroom_block" | "red_mushroom_block" => {
            let parts = [
                "all_inside",
                "north_west",
                "north",
                "north_east",
                "west",
                "center",
                "east",
                "south_west",
                "south",
                "south_east",
                "stem",
                "all_inside",
                "all_inside",
                "all_inside",
                "all_outside",
                "all_stem",
            ];
            state([prop("variant", parts[index])])
        }
        "tripwire_hook" => state([
            prop("attached", on(4)),
            prop("facing", facing),
            prop("powered", on(8)),
            prop("suspended", false),
        ]),
        "tripwire" => state(links().chain([prop("attached", on(4)), prop("suspended", on(2))])),
        "pumpkin_stem" | "melon_stem" => match form {
            0 => state([prop("age", meta & 7), prop("facing", "up")]),
            _ => state([prop("facing", ["north", "east", "south", "west"][form - 1])]),
        },
        "vine" => state([
            prop("east", on(8)),
            prop("north", on(4)),
            prop("south", on(1)),
            prop("up", bit(0)),
            prop("west", on(2)),
        ]),
        "fire" => state(links().chain([
            prop("alt", bit(4)),
            prop("flip", bit(5)),
            prop("upper", form >> 6),
        ])),
        "redstone_wire" => {
            state(SIDES.iter().zip(0..).map(|(side, index)| {
                prop(side, ["none", "side", "up"][form / 3usize.pow(index) % 3])
            }))
        }
        "flower_pot" => state([prop("contents", "empty")]),
        "cobblestone_wall" => {
            let wall = state(links().chain([prop("up", bit(4))]));
            if meta == 1 {
                wall.renamed("cobblestone_wall_", "mossy_wall")
            } else {
                wall
            }
        }
        "glass_pane" | "iron_bars" => state(links()),
        "stained_glass_pane" => state(links()).renamed("white_", color),
        _ if name.ends_with("fence") => state(links()),
        _ if name.ends_with("_stairs") => state([
            prop("facing", ["east", "west", "south", "north"][index & 3]),
            prop("half", if on(4) { "top" } else { "bottom" }),
            prop("shape", STAIR_SHAPES[form]),
        ]),
        _ if name.ends_with("fence_gate") => state([
            prop("facing", facing),
            prop("in_wall", bit(0)),
            prop("open", on(4)),
        ]),
        _ if name.ends_with("_door") => {
            let (lower, upper) = if on(8) { (form, index) } else { (index, form) };
            state([
                prop("facing", ["east", "south", "west", "north"][lower & 3]),
                prop("half", if on(8) { "upper" } else { "lower" }),
                prop("hinge", if upper & 1 == 1 { "right" } else { "left" }),
                prop("open", lower & 4 != 0),
            ])
        }
        _ => state([]),
    }
}

impl Variant {
    fn renamed(self, from: &'static str, to: &str) -> Variant {
        match self {
            Variant::State { key, .. } => Variant::State {
                key,
                rename: Some((from, format!("{to}_"))),
            },
            other => other,
        }
    }
}

fn pick<'a>(names: &[&'a str], index: usize, default: &'a str) -> &'a str {
    names.get(index).copied().unwrap_or(default)
}

fn model(name: impl Into<String>) -> Variant {
    Variant::Model(Pick {
        model: name.into(),
        ..Pick::default()
    })
}

fn rotated(name: &str, x: f32, y: f32) -> Variant {
    Variant::Model(Pick {
        model: name.into(),
        x,
        y,
        ..Pick::default()
    })
}

fn prop(name: &str, value: impl Display) -> String {
    format!("{name}={value}")
}

fn state(props: impl IntoIterator<Item = String>) -> Variant {
    let mut props: Vec<String> = props.into_iter().collect();
    props.sort();
    Variant::State {
        key: if props.is_empty() {
            "normal".into()
        } else {
            props.join(",")
        },
        rename: None,
    }
}
