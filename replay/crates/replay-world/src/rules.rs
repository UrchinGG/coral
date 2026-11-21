use crate::{Block, BlockModels, Flags, Form, Rule};

const SIDES: [[i32; 2]; 4] = [[0, -1], [1, 0], [0, 1], [-1, 0]];
const STAIRS_FACING: [usize; 4] = [1, 3, 2, 0];
const GATE_FACING: [usize; 4] = [2, 3, 0, 1];
const INNER: usize = 1;
const OUTER: usize = 3;

impl BlockModels {
    pub fn form(
        &self,
        state: u16,
        [x, y, z]: [i32; 3],
        at: impl Fn(i32, i32, i32) -> u16,
    ) -> &Form {
        let block = self.block(state);
        let meta = state & 15;
        let neighbor = |dx, dy, dz| self.block(at(dx, dy, dz));
        let side = |index: usize, dy| {
            let [dx, dz] = SIDES[index];
            neighbor(dx, dy, dz)
        };
        let links = |connects: &dyn Fn(Block) -> bool| {
            (0..4)
                .filter(|&index| connects(side(index, 0)))
                .map(|index| 1 << index)
                .sum::<usize>()
        };
        let attaches =
            |other: Block| other.flags.has(Flags::OCCLUDES) && !other.flags.has(Flags::GOURD);
        let stem = |fruit| {
            (meta & 7 == 7)
                .then(|| (0..4).rev().find(|&index| side(index, 0).flags.has(fruit)))
                .flatten()
                .map_or(0, |index| index + 1)
        };
        let index = match block.rule {
            Rule::Fixed => 0,
            Rule::Snowy => usize::from(neighbor(0, 1, 0).flags.has(Flags::SNOW)),
            Rule::DoublePlant => usize::from(at(0, -1, 0) & 7),
            Rule::Tripwire => links(&|other| other.flags.has(Flags::TRIPWIRE)),
            Rule::PumpkinStem => stem(Flags::PUMPKIN),
            Rule::MelonStem => stem(Flags::MELON),
            Rule::Vine => usize::from(neighbor(0, 1, 0).flags.has(Flags::OCCLUDES)),
            Rule::Fire => {
                let below = neighbor(0, -1, 0).flags;
                if below.has(Flags::SOLID) || below.has(Flags::FLAMMABLE) {
                    0
                } else {
                    let alt = ((x + y + z) & 1) as usize;
                    let flip = ((x.div_euclid(2) + y.div_euclid(2) + z.div_euclid(2)) & 1) as usize;
                    let upper = if neighbor(0, 1, 0).flags.has(Flags::FLAMMABLE) {
                        2 - alt
                    } else {
                        0
                    };
                    links(&|other| other.flags.has(Flags::FLAMMABLE))
                        | alt << 4
                        | flip << 5
                        | upper << 6
                }
            }
            Rule::Redstone => {
                let covered = neighbor(0, 1, 0).flags.has(Flags::OCCLUDES);
                (0..4).rev().fold(0, |form, index| {
                    let beside = side(index, 0);
                    let wire = |dy| side(index, dy).rule == Rule::Redstone;
                    let link = if !covered && beside.flags.has(Flags::OCCLUDES) && wire(1) {
                        2
                    } else {
                        usize::from(
                            beside.flags.has(Flags::REDSTONE)
                                || !beside.flags.has(Flags::OCCLUDES) && wire(-1),
                        )
                    };
                    form * 3 + link
                })
            }
            Rule::Wall => {
                let wall = links(&|other| {
                    matches!(other.rule, Rule::Wall | Rule::Gate) || attaches(other)
                });
                wall | usize::from(at(0, 1, 0) >> 4 != 0) << 4
            }
            Rule::Fence => links(&|other| match other.rule {
                Rule::Fence => {
                    other.flags.has(Flags::NETHER_FENCE) == block.flags.has(Flags::NETHER_FENCE)
                }
                Rule::Gate => true,
                _ => attaches(other),
            }),
            Rule::Pane => {
                links(&|other| other.flags.has(Flags::OCCLUDES) || other.flags.has(Flags::GLASS))
            }
            Rule::Stairs => self.stairs(meta, &at),
            Rule::Door => usize::from(if meta & 8 != 0 {
                at(0, -1, 0) & 7
            } else {
                at(0, 1, 0) & 1
            }),
            Rule::Gate => {
                let facing = GATE_FACING[usize::from(meta & 3)];
                usize::from(
                    [1, 3]
                        .iter()
                        .any(|turn| side((facing + turn) % 4, 0).rule == Rule::Wall),
                )
            }
        };
        &self.forms[block.first_form + index.min(block.rule.forms() - 1)]
    }

    fn stairs(&self, meta: u16, at: &impl Fn(i32, i32, i32) -> u16) -> usize {
        let stairs = |direction: usize| {
            let [dx, dz] = SIDES[direction];
            let state = at(dx, 0, dz);
            (self.block(state).rule == Rule::Stairs)
                .then(|| (STAIRS_FACING[usize::from(state & 3)], state & 4 != 0))
        };
        let facing = STAIRS_FACING[usize::from(meta & 3)];
        let top = meta & 4 != 0;
        let across = |direction| {
            stairs(direction)
                .filter(|&(other, half)| half == top && other % 2 != facing % 2)
                .map(|(other, _)| other)
        };
        let differs = |direction| stairs(direction) != Some((facing, top));
        let corner = match across(facing).filter(|&front| differs((front + 2) % 4)) {
            Some(front) => Some((OUTER, front)),
            None => across((facing + 2) % 4)
                .filter(|&back| differs(back))
                .map(|back| (INNER, back)),
        };
        corner.map_or(0, |(kind, turn)| {
            let left = (turn == (facing + 3) % 4) != top;
            kind + usize::from(!left)
        })
    }
}
