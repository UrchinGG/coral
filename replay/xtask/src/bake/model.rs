use std::{
    array::from_fn,
    collections::{BTreeMap, HashMap},
    error::Error,
    path::Path,
};

use replay_world::{Face, Quad};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct ModelFile {
    parent: Option<String>,
    #[serde(default)]
    textures: HashMap<String, String>,
    elements: Option<Vec<Element>>,
    ambientocclusion: Option<bool>,
}

#[derive(Deserialize)]
struct Element {
    from: [f32; 3],
    to: [f32; 3],
    rotation: Option<Rotation>,
    shade: Option<bool>,
    faces: BTreeMap<String, FaceSpec>,
}

#[derive(Deserialize)]
struct Rotation {
    origin: [f32; 3],
    axis: Axis,
    angle: f32,
    #[serde(default)]
    rescale: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Axis {
    X,
    Y,
    Z,
}

#[derive(Deserialize)]
struct FaceSpec {
    uv: Option<[f32; 4]>,
    texture: String,
    cullface: Option<String>,
    #[serde(default)]
    rotation: i32,
    tintindex: Option<i32>,
}

#[derive(Deserialize, Clone, Default)]
pub struct Pick {
    pub model: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub uvlock: bool,
    #[serde(skip)]
    pub retexture: Option<(String, String)>,
}

pub struct Models(HashMap<String, ModelFile>);

fn viewer_models() -> Value {
    let plank_box = |from: [f32; 3], to: [f32; 3]| {
        let faces: serde_json::Map<_, _> = ["down", "up", "north", "south", "west", "east"]
            .into_iter()
            .map(|face| (face.to_string(), json!({ "texture": "#planks" })))
            .collect();
        json!({ "from": from, "to": to, "faces": faces })
    };
    let sign = |elements: Vec<Value>| {
        json!({
            "ambientocclusion": false,
            "textures": { "planks": "blocks/planks_oak", "particle": "blocks/planks_oak" },
            "elements": elements,
        })
    };
    json!({
        "viewer_sign_standing": sign(vec![
            plank_box([0.0, 9.33, 7.33], [16.0, 17.33, 8.67]),
            plank_box([7.33, 0.0, 7.33], [8.67, 9.33, 8.67]),
        ]),
        "viewer_sign_wall": sign(vec![plank_box([0.0, 4.5, 14.67], [16.0, 12.5, 16.0])]),
    })
}

impl Models {
    pub fn load(path: &Path) -> Result<Models, Box<dyn Error>> {
        let mut files: HashMap<String, ModelFile> = super::read_json(path)?;
        files.extend(serde_json::from_value::<HashMap<_, _>>(viewer_models())?);
        Ok(Models(files))
    }

    pub fn bake(&self, pick: &Pick, mut layer: impl FnMut(&str) -> u16) -> Option<Vec<Quad>> {
        let (chain, textures) = self.chain(pick)?;
        let elements = chain
            .iter()
            .find_map(|model| model.elements.as_deref())
            .unwrap_or_default();
        let smooth = chain
            .iter()
            .find_map(|model| model.ambientocclusion)
            .unwrap_or(true);

        let mut quads = Vec::new();
        for element in elements {
            for (name, spec) in &element.faces {
                let (Some(side), Some(texture)) = (face(name), lookup(&textures, &spec.texture))
                else {
                    continue;
                };
                if matches!(texture, "missingno" | "grass_side_overlay") {
                    continue;
                }
                let texture = match &pick.retexture {
                    Some((from, to)) if from == texture => to,
                    _ => texture,
                };
                let corners = face_corners(side, element.from, element.to);
                let placed = corners.map(|point| {
                    pick.place(
                        element
                            .rotation
                            .as_ref()
                            .map_or(point, |rotation| rotation.apply(point)),
                    )
                });
                let direction = pick.turn(side);
                let uv = if pick.uvlock {
                    placed.map(|point| default_uv(direction, point))
                } else {
                    let [u1, v1, u2, v2] = spec.uv.unwrap_or_else(|| {
                        let ([u1, v1], [u2, v2]) =
                            (default_uv(side, corners[0]), default_uv(side, corners[2]));
                        [u1, v1, u2, v2]
                    });
                    let square = [[u1, v1], [u1, v2], [u2, v2], [u2, v1]];
                    let turns = (spec.rotation / 90).rem_euclid(4) as usize;
                    from_fn(|corner| square[(corner + turns) % 4])
                };
                quads.push(Quad {
                    corners: placed.map(|point| point.map(|value| value / 16.0)),
                    uv: uv.map(|point| point.map(|value| value / 16.0)),
                    layer: layer(texture),
                    face: direction,
                    cull: spec
                        .cullface
                        .as_deref()
                        .and_then(face)
                        .map(|cull| pick.turn(cull)),
                    tinted: spec.tintindex.is_some(),
                    shade: element.shade != Some(false),
                    smooth,
                });
            }
        }
        Some(quads)
    }

    pub fn particle(&self, pick: &Pick) -> Option<&str> {
        let (_, textures) = self.chain(pick)?;
        lookup(&textures, "#particle").filter(|&texture| texture != "missingno")
    }

    fn chain(&self, pick: &Pick) -> Option<(Vec<&ModelFile>, HashMap<&str, &str>)> {
        let mut chain = Vec::new();
        let mut next = Some(pick.model.as_str());
        while let Some(name) = next
            && chain.len() < 16
            && let Some(model) = self.0.get(name.trim_start_matches("block/"))
        {
            chain.push(model);
            next = model.parent.as_deref();
        }
        let textures = chain
            .iter()
            .rev()
            .flat_map(|model| &model.textures)
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        (!chain.is_empty()).then_some((chain, textures))
    }
}

impl Pick {
    fn place(&self, point: [f32; 3]) -> [f32; 3] {
        rotate(
            rotate(point, Axis::X, -self.x, [8.0; 3]),
            Axis::Y,
            -self.y,
            [8.0; 3],
        )
    }

    fn turn(&self, face: Face) -> Face {
        let normal = self
            .place(face.normal().map(|value| value as f32 * 8.0 + 8.0))
            .map(|value| value - 8.0);
        let axis = (0..3)
            .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
            .unwrap_or_default();
        Face::ALL[axis * 2 + usize::from(normal[axis] < 0.0)]
    }
}

impl Rotation {
    fn apply(&self, point: [f32; 3]) -> [f32; 3] {
        let rotated = rotate(point, self.axis, self.angle, self.origin);
        let scale = if self.rescale {
            1.0 / self.angle.to_radians().cos()
        } else {
            1.0
        };
        from_fn(|axis| {
            if axis == self.axis as usize {
                rotated[axis]
            } else {
                self.origin[axis] + (rotated[axis] - self.origin[axis]) * scale
            }
        })
    }
}

fn rotate(point: [f32; 3], axis: Axis, degrees: f32, origin: [f32; 3]) -> [f32; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let [x, y, z]: [f32; 3] = from_fn(|axis| point[axis] - origin[axis]);
    let rotated = match axis {
        Axis::X => [x, y * cos - z * sin, y * sin + z * cos],
        Axis::Y => [x * cos + z * sin, y, -x * sin + z * cos],
        Axis::Z => [x * cos - y * sin, x * sin + y * cos, z],
    };
    from_fn(|axis| rotated[axis] + origin[axis])
}

fn lookup<'a>(textures: &HashMap<&str, &'a str>, reference: &'a str) -> Option<&'a str> {
    let mut value = reference;
    for _ in 0..8 {
        let Some(key) = value.strip_prefix('#') else {
            break;
        };
        value = textures.get(key)?;
    }
    (!value.starts_with('#')).then(|| value.rsplit_once('/').map_or(value, |(_, name)| name))
}

fn face(name: &str) -> Option<Face> {
    Some(match name {
        "east" => Face::East,
        "west" => Face::West,
        "up" => Face::Up,
        "down" => Face::Down,
        "south" => Face::South,
        "north" => Face::North,
        _ => return None,
    })
}

fn face_corners(face: Face, [x0, y0, z0]: [f32; 3], [x1, y1, z1]: [f32; 3]) -> [[f32; 3]; 4] {
    match face {
        Face::Up => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        Face::Down => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        Face::North => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        Face::South => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        Face::West => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        Face::East => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
    }
}

fn default_uv(face: Face, [x, y, z]: [f32; 3]) -> [f32; 2] {
    match face {
        Face::Up => [x, z],
        Face::Down => [x, 16.0 - z],
        Face::North => [16.0 - x, 16.0 - y],
        Face::South => [x, 16.0 - y],
        Face::West => [z, 16.0 - y],
        Face::East => [16.0 - z, 16.0 - y],
    }
}
