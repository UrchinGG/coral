# replay

The Urchin replay viewer: a Rust engine that decodes Minecraft 1.8.9 recordings in the
browser and renders them with wgpu (WebGPU, with a WebGL2 fallback). The page around it
lives in `web/sites/urchin/src/pages/replays/`.

This is its own Cargo workspace, so its wasm build and dependencies stay separate from
the coral services.

## Crates

```
crates/replay-file      opens .mcpr (ReplayMod) and URP1 archives into packets
crates/replay-protocol  decodes protocol-47 clientbound packets
crates/replay-model     turns packets into one immutable Replay (clock, world, players, events)
crates/replay-assets    baked asset tables: items, entity atlas, glyphs, and their binary codec
crates/replay-world     block models, seekable world state and the mesher
crates/replay-sim       player motion, swing timing, limb poses, particles and sound scheduling
crates/replay-render    the wgpu renderer
crates/replay-viewer    the wasm-bindgen API used by the page
xtask                   developer commands: inspect, mesh, bake-assets, bake-sounds
```

## Commands

Run these from `replay/`:

- `cargo run -p xtask -- inspect <file>` prints what the model reads from a recording
- `cargo run --release -p xtask -- mesh <file> <assets-dir>` meshes the map and times it
- `cargo run -p xtask -- bake-assets <out-dir>` bakes textures and model tables from `assets-src/`, each with a gzipped `.gz` copy
- `cargo run -p xtask -- bake-sounds <out-dir>` downloads the 1.8.9 sounds from Mojang into `<out-dir>/sounds/`

`npm run build:viewer` in `web/` builds the wasm into `sites/urchin/src/viewer/pkg`.

`assets-src/` holds the 1.8 textures and tables from the `minecraft-assets` and
`minecraft-data` npm packages and the mctext fonts. It is not committed. Its baked output
is, in `web/sites/urchin/public/replay-assets`, so bake into that folder to change what
the site serves.
