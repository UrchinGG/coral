struct Globals {
    view_proj: mat4x4<f32>,
    camera_block: vec3<i32>,
    fog_start: f32,
    camera_fract: vec3<f32>,
    fog_end: f32,
    glint: vec4<f32>,
    time: f32,
    background: vec4<f32>,
    focus: vec4<f32>,
    lightmap: array<vec4<f32>, 256>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var blocks: texture_2d_array<f32>;
@group(0) @binding(2) var blocks_sampler: sampler;
@group(0) @binding(3) var sheet: texture_2d<f32>;
@group(0) @binding(4) var skins: texture_2d_array<f32>;
@group(0) @binding(5) var pixel_sampler: sampler;
@group(0) @binding(6) var entity_sampler: sampler;
@group(1) @binding(0) var<uniform> section: vec4<i32>;

override ALPHA_TO_COVERAGE: bool = false;

const GAMMA = 2.2;
const CUTOFF = 0.1;
const SHEET = -1;
const PLAIN = -2;

const ARMOR_GLINT = vec3<f32>(0.38, 0.19, 0.608);
const ITEM_GLINT = vec3<f32>(128.0, 64.0, 204.0) / 255.0;
const ARMOR_TEXTURE = vec2<f32>(64.0, 32.0);
const TICK_MS = 50.0;
const REVEAL_HALF_HEIGHT = 0.75;
const REVEAL_RADIUS = vec2<f32>(0.2, 1.0);
const REVEAL_DEPTH = vec2<f32>(0.2, 0.7);
const REVEAL_CELL = 7.0;

fn linear(color: vec3<f32>) -> vec3<f32> {
    return pow(color, vec3<f32>(GAMMA));
}

fn fog_amount(relative: vec3<f32>) -> f32 {
    return clamp((length(relative) - globals.fog_start) / (globals.fog_end - globals.fog_start), 0.0, 1.0);
}

fn fogged(color: vec3<f32>, relative: vec3<f32>) -> vec3<f32> {
    return mix(color, globals.background.rgb, fog_amount(relative));
}

fn lightmap_entry(sky: i32, block: i32) -> vec3<f32> {
    return globals.lightmap[sky * 16 + block].rgb;
}

fn lightmap(levels: vec2<f32>) -> vec3<f32> {
    let scaled = levels * 15.0;
    let low = clamp(floor(scaled), vec2<f32>(0.0), vec2<f32>(14.0));
    let blend = scaled - low;
    let at = vec2<i32>(low);
    let dim = mix(lightmap_entry(at.x, at.y), lightmap_entry(at.x, at.y + 1), blend.y);
    let bright = mix(lightmap_entry(at.x + 1, at.y), lightmap_entry(at.x + 1, at.y + 1), blend.y);
    return linear(mix(dim, bright, blend.x));
}

fn coverage(alpha: f32) -> f32 {
    if ALPHA_TO_COVERAGE {
        return saturate((alpha - CUTOFF) / max(fwidth(alpha), 0.0001) + 0.5);
    }
    if alpha < CUTOFF {
        discard;
    }
    return 1.0;
}

fn block_texel(uv: vec2<f32>, layer: i32) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(blocks));
    let texel = uv * size;
    let seam = floor(texel + 0.5);
    let blur = clamp(fwidth(texel), vec2<f32>(0.0001), vec2<f32>(1.0));
    let crisp = (seam + clamp((texel - seam) / blur, vec2<f32>(-0.5), vec2<f32>(0.5))) / size;
    return textureSampleGrad(blocks, blocks_sampler, crisp, layer, dpdx(uv), dpdy(uv));
}

fn relative_to_camera(origin: vec3<i32>, offset: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(origin - globals.camera_block) - globals.camera_fract + offset;
}

struct TerrainIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
    @location(3) light: vec4<f32>,
    @location(4) tint: vec4<f32>,
}

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) relative: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) light: vec4<f32>,
    @location(4) tint: vec4<f32>,
}

@vertex
fn vs_terrain(in: TerrainIn) -> TerrainOut {
    let relative = relative_to_camera(section.xyz * 16, in.position);
    return TerrainOut(globals.view_proj * vec4<f32>(relative, 1.0), relative, in.uv, in.layer, in.light, in.tint.wzyx);
}

fn terrain_color(in: TerrainOut) -> vec4<f32> {
    let texel = block_texel(in.uv, i32(in.layer));
    let lit = texel.rgb * linear(in.tint.rgb) * pow(in.light.x, GAMMA) * lightmap(in.light.yz);
    return vec4<f32>(fogged(lit, in.relative), texel.a * in.tint.a);
}

fn reveal(pixel: vec2<f32>, relative: vec3<f32>) {
    if globals.focus.w <= 0.0 {
        return;
    }
    let focus = globals.focus.xyz;
    let ray = normalize(relative);
    let crossing = ray * dot(focus, focus) / dot(ray, focus) - focus;
    let gap = vec3<f32>(crossing.x, crossing.y - clamp(crossing.y, -REVEAL_HALF_HEIGHT, REVEAL_HALF_HEIGHT), crossing.z);
    let nearer = length(focus) - length(relative);
    let strength = globals.focus.w;
    let radius = REVEAL_RADIUS * max(strength, 1.0);
    let open = min(strength, 1.0)
        * (1.0 - smoothstep(radius.x, radius.y, length(gap)))
        * smoothstep(REVEAL_DEPTH.x, REVEAL_DEPTH.y, nearer);
    let cell = fract(pixel / REVEAL_CELL) - 0.5;
    if length(cell) > (1.0 - open) * 0.75 {
        discard;
    }
}

@fragment
fn fs_opaque(in: TerrainOut) -> @location(0) vec4<f32> {
    reveal(in.clip.xy, in.relative);
    let color = terrain_color(in);
    return vec4<f32>(color.rgb, coverage(color.a));
}

@fragment
fn fs_translucent(in: TerrainOut) -> @location(0) vec4<f32> {
    reveal(in.clip.xy, in.relative);
    return terrain_color(in);
}


const TRIANGLE = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
const CUBOID_CORNERS = array<u32, 24>(
    5u, 1u, 3u, 7u, 0u, 4u, 6u, 2u, 5u, 4u, 0u, 1u,
    3u, 2u, 6u, 7u, 1u, 0u, 2u, 3u, 4u, 5u, 7u, 6u,
);
const CUBOID_NORMALS = array<vec3<f32>, 6>(
    vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(-1.0, 0.0, 0.0),
    vec3<f32>(0.0, -1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, 0.0, -1.0), vec3<f32>(0.0, 0.0, 1.0),
);

struct CuboidIn {
    @location(0) column0: vec4<f32>,
    @location(1) column1: vec4<f32>,
    @location(2) column2: vec4<f32>,
    @location(3) column3: vec4<f32>,
    @location(4) origin: vec3<f32>,
    @location(5) inflate: f32,
    @location(6) size: vec3<f32>,
    @location(7) layer: i32,
    @location(8) uv: vec2<f32>,
    @location(9) mirror: u32,
    @location(10) tint: vec4<f32>,
    @location(11) flash: vec4<f32>,
}

struct CuboidOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) relative: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) uv: vec2<f32>,
    @location(2) @interpolate(flat) layer: i32,
    @location(3) tint: vec4<f32>,
    @location(4) flash: vec4<f32>,
    @location(5) shade: f32,
}

fn cuboid_rect(face: u32, size: vec3<f32>) -> vec4<f32> {
    let w = size.x;
    let h = size.y;
    let d = size.z;
    switch face {
        case 0u: { return vec4<f32>(d + w, d, d + w + d, d + h); }
        case 1u: { return vec4<f32>(0.0, d, d, d + h); }
        case 2u: { return vec4<f32>(d, 0.0, d + w, d); }
        case 3u: { return vec4<f32>(d + w, d, d + w + w, 0.0); }
        case 4u: { return vec4<f32>(d, d, d + w, d + h); }
        default: { return vec4<f32>(d + w + d, d, d + w + d + w, d + h); }
    }
}

@vertex
fn vs_cuboid(@builtin(vertex_index) index: u32, in: CuboidIn) -> CuboidOut {
    let face = index / 6u;
    let corner = TRIANGLE[index % 6u];
    let bits = CUBOID_CORNERS[face * 4u + corner];
    var high = vec3<f32>(f32(bits & 1u), f32((bits >> 1u) & 1u), f32(bits >> 2u));
    if in.mirror != 0u {
        high.x = 1.0 - high.x;
    }
    let local = mix(in.origin - in.inflate, in.origin + in.size + in.inflate, high);
    let transform = mat4x4<f32>(in.column0, in.column1, in.column2, in.column3);
    let normal = normalize((transform * vec4<f32>(CUBOID_NORMALS[face], 0.0)).xyz);
    let shade = abs(normal.x) * 0.6 + abs(normal.z) * 0.8 + select(-normal.y * 0.5, normal.y, normal.y > 0.0);
    let rect = cuboid_rect(face, in.size);
    let corner_uv = vec2<f32>(
        select(rect.x, rect.z, corner == 0u || corner == 3u),
        select(rect.w, rect.y, corner < 2u),
    );
    let relative = relative_to_camera(vec3<i32>(0), (transform * vec4<f32>(local, 1.0)).xyz);
    return CuboidOut(
        globals.view_proj * vec4<f32>(relative, 1.0),
        relative,
        in.uv + corner_uv,
        in.layer,
        in.tint.wzyx,
        in.flash.wzyx,
        pow(clamp(shade, 0.0, 1.0), GAMMA),
    );
}

@fragment
fn fs_cuboid(in: CuboidOut) -> @location(0) vec4<f32> {
    let skin_uv = in.uv / vec2<f32>(textureDimensions(skins));
    let sheet_uv = in.uv / vec2<f32>(textureDimensions(sheet));
    let layer = max(in.layer, 0);
    let on_sheet = in.layer == SHEET;
    let texel = select(
        textureSample(skins, entity_sampler, skin_uv, layer),
        textureSample(sheet, entity_sampler, sheet_uv),
        on_sheet,
    );
    let texel_at = vec2<i32>(floor(in.uv));
    let mask = select(
        textureLoad(skins, min(texel_at, vec2<i32>(textureDimensions(skins)) - 1), layer, 0),
        textureLoad(sheet, min(texel_at, vec2<i32>(textureDimensions(sheet)) - 1), 0),
        on_sheet,
    );
    if mask.a < CUTOFF {
        discard;
    }
    let lit = mix(texel.rgb * linear(in.tint.rgb) * in.shade, in.flash.rgb, in.flash.a);
    return vec4<f32>(fogged(lit, in.relative), coverage(in.tint.a));
}

struct MeshIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: i32,
    @location(3) tint: vec4<f32>,
}

struct MeshOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) relative: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) uv: vec2<f32>,
    @location(2) @interpolate(flat) layer: i32,
    @location(3) tint: vec4<f32>,
}

@vertex
fn vs_mesh(in: MeshIn) -> MeshOut {
    let relative = relative_to_camera(vec3<i32>(0), in.position);
    return MeshOut(globals.view_proj * vec4<f32>(relative, 1.0), relative, in.uv, in.layer, in.tint.wzyx);
}

fn mesh_color(in: MeshOut) -> vec4<f32> {
    let block = block_texel(in.uv, max(in.layer, 0));
    let sheet_texel = textureSampleLevel(sheet, pixel_sampler, in.uv / vec2<f32>(textureDimensions(sheet)), 0.0);
    let texel = select(select(block, sheet_texel, in.layer == SHEET), vec4<f32>(1.0), in.layer == PLAIN);
    return texel * vec4<f32>(linear(in.tint.rgb), in.tint.a);
}

@fragment
fn fs_mesh(in: MeshOut) -> @location(0) vec4<f32> {
    let color = mesh_color(in);
    return vec4<f32>(fogged(color.rgb, in.relative), coverage(color.a));
}

@fragment
fn fs_blended(in: MeshOut) -> @location(0) vec4<f32> {
    let color = mesh_color(in);
    return vec4<f32>(fogged(color.rgb, in.relative), color.a);
}

@fragment
fn fs_crack(in: MeshOut) -> @location(0) vec4<f32> {
    let texel = block_texel(in.uv, in.layer);
    if texel.a < CUTOFF {
        discard;
    }
    let factor = min(texel.rgb * pow(2.0, GAMMA - 1.0), vec3<f32>(1.0));
    return vec4<f32>(mix(factor, vec3<f32>(0.5), fog_amount(in.relative)), 1.0);
}

fn turn(degrees: f32) -> mat2x2<f32> {
    let angle = radians(degrees);
    return mat2x2<f32>(cos(angle), sin(angle), -sin(angle), cos(angle));
}

fn glint(uv: vec2<f32>, color: vec3<f32>) -> vec3<f32> {
    let texel = fract(uv) * globals.glint.zw + globals.glint.xy;
    let sampled = textureSampleLevel(sheet, pixel_sampler, texel / vec2<f32>(textureDimensions(sheet)), 0.0);
    let shade = pow(sampled.rgb, vec3<f32>(1.0 / GAMMA)) * color;
    return shade * shade;
}

@fragment
fn fs_armor_glint(in: CuboidOut) -> @location(0) vec4<f32> {
    let uv = in.uv / ARMOR_TEXTURE;
    let ticks = globals.time / TICK_MS;
    let first = turn(30.0) * (uv + vec2<f32>(0.0, ticks * 0.02)) / 3.0;
    let second = turn(-30.0) * (uv + vec2<f32>(0.0, ticks * 0.08)) / 3.0;
    return vec4<f32>(glint(first, ARMOR_GLINT) + glint(second, ARMOR_GLINT), 0.0);
}

@fragment
fn fs_item_glint(in: MeshOut) -> @location(0) vec4<f32> {
    let uv = in.uv / 4.0;
    let first = turn(-50.0) * uv + vec2<f32>(fract(globals.time / 3000.0), 0.0);
    let second = turn(10.0) * uv - vec2<f32>(fract(globals.time / 4873.0), 0.0);
    return vec4<f32>(glint(first, ITEM_GLINT) + glint(second, ITEM_GLINT), 0.0);
}

struct BillboardIn {
    @location(0) center: vec3<f32>,
    @location(1) size: f32,
    @location(2) uv: vec4<f32>,
    @location(3) layer: i32,
    @location(4) tint: vec4<f32>,
    @location(5) flat: u32,
}

@vertex
fn vs_billboard(@builtin(vertex_index) index: u32, in: BillboardIn) -> MeshOut {
    let corner = TRIANGLE[index];
    let quad = vec2<f32>(f32(corner == 1u || corner == 2u), f32(corner >= 2u));
    let offset = (quad * 2.0 - 1.0) * in.size;
    let view = globals.view_proj;
    let right = normalize(vec3<f32>(view[0].x, view[1].x, view[2].x));
    let up = normalize(vec3<f32>(view[0].y, view[1].y, view[2].y));
    let facing = select(right * offset.x + up * offset.y, vec3<f32>(offset.x, 0.0, -offset.y), in.flat != 0u);
    let relative = relative_to_camera(vec3<i32>(0), in.center + facing);
    let uv = vec2<f32>(mix(in.uv.x, in.uv.z, quad.x), mix(in.uv.w, in.uv.y, quad.y));
    return MeshOut(globals.view_proj * vec4<f32>(relative, 1.0), relative, uv, in.layer, in.tint.wzyx);
}

@fragment
fn fs_particle(in: MeshOut) -> @location(0) vec4<f32> {
    let color = mesh_color(in);
    if color.a < 0.02 {
        discard;
    }
    return vec4<f32>(fogged(color.rgb, in.relative), color.a);
}
