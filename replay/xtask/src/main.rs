mod bake;
mod sounds;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    path::Path,
    process::ExitCode,
    time::Instant,
};

use replay_file::Recording;
use replay_model::{EffectKind, EventKind, Replay};
use replay_sim::Players;
use replay_world::{BlockModels, Lighting, WorldState, mesh_section};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [command, path] if command == "inspect" => inspect(path),
        [command, out] if command == "bake-assets" => bake::bake_assets(Path::new(out)),
        [command, out] if command == "bake-sounds" => sounds::bake_sounds(Path::new(out)),
        [command, path, assets] if command == "mesh" => mesh(path, Path::new(assets)),
        [command, path, player, times @ ..] if command == "frames" => frames(path, player, times),
        _ => Err("usage: cargo xtask inspect <recording> | bake-assets <out-dir> | bake-sounds <out-dir> | mesh <recording> <assets-dir> | frames <recording> <player> <ms>...".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn inspect(path: &str) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let opening = Instant::now();
    let recording = Recording::open(&bytes)?;
    let building = Instant::now();
    let replay = replay_model::build(&recording);
    let built = Instant::now();

    let info = &replay.info;
    println!("duration       {}", clock(info.duration));
    println!("playback start {} ms into the recording", info.start);
    println!(
        "server         {}",
        info.server_name.as_deref().unwrap_or("-")
    );
    for (label, value) in [
        ("game", &info.game),
        ("mode", &info.mode),
        ("map", &info.map),
        ("date", &info.date),
        ("time", &info.time),
    ] {
        println!("{label:<14} {}", value.as_deref().unwrap_or("-"));
    }

    println!("players        {}", replay.players.len());
    for (index, player) in replay.players.iter().enumerate() {
        println!(
            "  {index:>2} #{:06x} {:<16} moves {:>6} equipment {:>4} skin {}",
            player.team_color,
            player.username,
            player.moves.t.len(),
            player.equipment.len(),
            if player.slim { "slim" } else { "classic" },
        );
    }

    let world = &replay.world;
    println!(
        "world at t=0   {} columns, {} sections",
        columns(world.initial.iter().map(|&(pos, _)| pos)),
        world.initial.len()
    );
    println!(
        "timed loads    {} sections in {} columns",
        world.loads.t.len(),
        columns(world.loads.pos.iter().copied())
    );
    println!("block changes  {}", world.changes.t.len());

    print_events(&replay);
    print_tally(
        "objects",
        replay
            .objects
            .iter()
            .map(|object| format!("{:?}", object.kind)),
    );
    print_tally(
        "effects",
        replay.effects.iter().map(|effect| {
            match effect.kind {
                EffectKind::Particle { .. } => "particle",
                EffectKind::Sound { .. } => "sound",
                EffectKind::World { .. } => "world",
                EffectKind::Lid { .. } => "lid",
                EffectKind::Crit { .. } => "crit",
                EffectKind::Explosion { .. } => "explosion",
            }
            .to_owned()
        }),
    );
    println!("items          {}", replay.items.len());
    println!(
        "open {} ms, build {} ms",
        (building - opening).as_millis(),
        (built - building).as_millis()
    );
    Ok(())
}

fn mesh(path: &str, assets: &Path) -> Result<(), Box<dyn Error>> {
    let recording = Recording::open(&fs::read(path)?)?;
    let models = BlockModels::decode(&fs::read(assets.join("blocks.bin"))?)
        .ok_or("unreadable blocks.bin")?;
    let mut world = WorldState::new(replay_model::build(&recording).world);
    world.seek(0);
    let sections: Vec<_> = world.drain_dirty().collect();

    let lighting_started = Instant::now();
    let mut lighting = Lighting::new(&models);
    for &section in &sections {
        lighting.prepare(&world, section);
    }
    let lit = lighting_started.elapsed();

    let meshing = Instant::now();
    let meshes: Vec<_> = sections
        .iter()
        .map(|&section| mesh_section(&world, &lighting, &models, section))
        .collect();
    let elapsed = meshing.elapsed();

    let count = |geometry: fn(&replay_world::SectionMesh) -> &replay_world::Geometry| {
        meshes
            .iter()
            .map(geometry)
            .fold((0, 0), |(vertices, indices), geometry| {
                (
                    vertices + geometry.vertices.len(),
                    indices + geometry.indices.len(),
                )
            })
    };
    let (opaque_vertices, opaque_indices) = count(|mesh| &mesh.opaque);
    let (translucent_vertices, translucent_indices) = count(|mesh| &mesh.translucent);
    println!("sections     {}", sections.len());
    println!("lighting     {:.1} ms", lit.as_secs_f64() * 1000.0);
    println!("opaque       {opaque_vertices} vertices, {opaque_indices} indices");
    println!("translucent  {translucent_vertices} vertices, {translucent_indices} indices");
    println!(
        "meshing      {:.1} ms ({:.1} us per section)",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1e6 / sections.len().max(1) as f64
    );
    Ok(())
}

fn frames(path: &str, player: &str, times: &[String]) -> Result<(), Box<dyn Error>> {
    let replay = replay_model::build(&Recording::open(&fs::read(path)?)?);
    let index: usize = player.parse()?;
    let username = &replay.players.get(index).ok_or("no such player")?.username;
    let simulating = Instant::now();
    let players = Players::new(&replay);
    println!(
        "{username}, simulated in {} ms",
        simulating.elapsed().as_millis()
    );
    println!(
        "        t  x         y        z           yaw    head    body   pitch  limb    amt  flags held  swing  hurt  hand"
    );
    for time in times {
        let t: f32 = time.parse()?;
        let Some(frame) = players.sample(index, t) else {
            println!("{:>9}  absent", clock(t as u32));
            continue;
        };
        let progress = |value: Option<f32>| value.map_or("-".into(), |value| format!("{value:.2}"));
        let held = frame
            .equipment
            .held
            .map(|item| replay.items[item as usize].id);
        let hand = (frame.pose.root * frame.pose.held_item).w_axis.truncate() - frame.position;
        let finite = hand.is_finite() && frame.pose.head.is_finite();
        println!(
            "{:>9}  {:>9.3} {:>8.3} {:>9.3}  {:>6.1}  {:>6.1}  {:>6.1}  {:>6.1}  {:>5.2}  {:>5.2}  {:>#04x}  {:>4}  {:>5}  {:>4}  ({:.2}, {:.2}, {:.2}){}",
            clock(t as u32),
            frame.position.x,
            frame.position.y,
            frame.position.z,
            frame.yaw.to_degrees(),
            frame.head_yaw.to_degrees(),
            frame.body_yaw.to_degrees(),
            frame.pitch.to_degrees(),
            frame.limb_swing,
            frame.limb_amount,
            frame.state.flags,
            held.map_or("-".into(), |id| id.to_string()),
            progress(frame.swing),
            progress(frame.hurt),
            hand.x,
            hand.y,
            hand.z,
            if finite { "" } else { "  NOT FINITE" },
        );
    }
    Ok(())
}

fn print_events(replay: &Replay) {
    let count = |matches: fn(&EventKind) -> bool| {
        replay
            .events
            .iter()
            .filter(|event| matches(&event.kind))
            .count()
    };
    let chat: Vec<_> = replay
        .events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::Chat { text, .. } => Some((event.t, text)),
            _ => None,
        })
        .collect();
    println!("chat           {}", chat.len());
    for (t, text) in chat.iter().take(5) {
        println!("  {} {text}", clock(*t));
    }
    println!(
        "block_break {}, swing {}, hurt {}, death {}, collect {}",
        count(|kind| matches!(kind, EventKind::BlockBreak { .. })),
        count(|kind| matches!(kind, EventKind::Swing(_))),
        count(|kind| matches!(kind, EventKind::Hurt(_))),
        count(|kind| matches!(kind, EventKind::Dead(_))),
        count(|kind| matches!(kind, EventKind::Collect { .. })),
    );
}

fn print_tally(label: &str, keys: impl Iterator<Item = String>) {
    let mut counts = HashMap::new();
    for key in keys {
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    let total: usize = counts.iter().map(|&(_, count)| count).sum();
    println!("{label:<14} {total}");
    for (key, count) in counts {
        println!("  {count:>6} {key}");
    }
}

fn columns(sections: impl Iterator<Item = [i32; 3]>) -> usize {
    sections
        .map(|[x, _, z]| [x, z])
        .collect::<HashSet<_>>()
        .len()
}

fn clock(ms: u32) -> String {
    format!("{}:{:02}.{:03}", ms / 60000, ms / 1000 % 60, ms % 1000)
}
