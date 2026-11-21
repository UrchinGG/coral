#![cfg(target_arch = "wasm32")]

mod assets;
mod chests;
mod controls;
mod cracks;
mod draw;
mod effects;
mod info;
mod items;
mod mobs;
mod model;
mod objects;
mod players;
mod skin;
mod text;

use std::collections::HashSet;

use assets::Assets;
use chests::Chests;
use controls::{Area, Controls, EYE_HEIGHT, Listener, Mode, Ray};
use cracks::Cracks;
use draw::Draw;
use glam::{IVec3, Vec2, Vec3};
use objects::Objects;
use players::Person;
use replay_file::Recording;
use replay_model::{Item, Replay};
use replay_render::{Camera, Daylight, Renderer, Scene, SurfaceTarget, Tier, WHITE};
use replay_sim::{Origin, Particles, PlayerFrame, Players, Sounds};
use replay_world::{BlockModels, Flags, Lighting, WorldState, mesh_section};
use wasm_bindgen::prelude::*;
use web_sys::{Event, HtmlCanvasElement, KeyboardEvent, MouseEvent, PointerEvent, WheelEvent};

const MAX_FRAME_MS: f64 = 50.0;
const REVEAL_HEIGHT: f32 = 1.0;
const MESH_BUDGET_MS: f64 = 4.0;
const SPEED_RANGE: [f64; 2] = [0.1, 8.0];
const PICK_STEP: f32 = 0.25;
const PICK_HEIGHTS: [f32; 3] = [0.3, 1.0, 1.7];
const MAX_AUDIBLE_SPEED: f64 = 4.0;
const MAX_SOUNDS_PER_FRAME: usize = 24;
const FAR_SOUND_DISTANCE: f32 = 16.0;
const HEARING: f32 = 16.0;

#[wasm_bindgen]
pub struct Viewer {
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    assets: Option<Assets>,
    session: Option<Session>,
    scene: Scene,
    controls: Controls,
    followed: Option<u32>,
    last_frame: Option<f64>,
    time: f64,
    duration: f64,
    speed: f64,
    playing: bool,
    heard: Vec<f32>,
}

struct Session {
    players: Players,
    people: Vec<Person>,
    items: Vec<Item>,
    skulls: Vec<i32>,
    objects: Objects,
    particles: Particles,
    sounds: Sounds,
    chests: Chests,
    cracks: Cracks,
    world: WorldState,
    lighting: Lighting,
    daytime: Vec<(u32, u16)>,
    pending: HashSet<[i32; 3]>,
}

#[wasm_bindgen]
impl Viewer {
    pub async fn create(canvas: HtmlCanvasElement, web_gpu: bool) -> Result<Viewer, JsError> {
        std::panic::set_hook(Box::new(|info| {
            web_sys::console::error_1(&info.to_string().into())
        }));
        let target = SurfaceTarget::Canvas(canvas.clone());
        let renderer = Renderer::new(target, canvas.width(), canvas.height(), web_gpu).await?;
        Ok(Viewer {
            controls: Controls::new(canvas.client_width() as f32, canvas.client_height() as f32),
            canvas,
            renderer,
            assets: None,
            session: None,
            scene: Scene::default(),
            followed: None,
            last_frame: None,
            time: 0.0,
            duration: 0.0,
            speed: 1.0,
            playing: false,
            heard: Vec::new(),
        })
    }

    #[wasm_bindgen(js_name = loadAssets)]
    pub fn load_assets(&mut self, files: &JsValue) -> Result<(), JsError> {
        self.assets = Some(Assets::load(files, &mut self.renderer)?);
        Ok(())
    }

    #[wasm_bindgen(unchecked_return_type = "ReplayInfo")]
    pub fn load(&mut self, bytes: &[u8]) -> Result<JsValue, JsError> {
        let assets = self
            .assets
            .as_ref()
            .ok_or_else(|| JsError::new("assets are still loading"))?;
        let replay = replay_model::build(&Recording::open(bytes)?);
        let (people, skulls, skins) = allocate_skins(&replay, &mut self.renderer, assets);
        let info = info::json(&replay, &skins);
        let sections = replay.world.initial.iter().map(|&(pos, _)| pos.into());
        self.controls.frame_map(Area::new(sections));
        self.renderer.clear_sections();
        let players = Players::new(&replay);
        let chests = Chests::new(&replay.effects);
        let cracks = Cracks::new(&replay.events);
        let particles = Particles::new(&replay);
        let sounds = Sounds::new(&replay);
        let world = WorldState::new(replay.world);
        let lighting = Lighting::new(&assets.models);
        let mut session = Session {
            particles,
            sounds,
            players,
            people,
            skulls,
            chests,
            cracks,
            items: replay.items,
            objects: Objects::new(replay.objects),
            world,
            lighting,
            daytime: replay.daytime,
            pending: HashSet::new(),
        };
        session.refresh();
        self.session = Some(session);
        self.followed = None;
        self.time = 0.0;
        self.duration = f64::from(replay.info.duration);
        self.playing = false;
        js_sys::JSON::parse(&info).map_err(|_| JsError::new("unserializable replay info"))
    }

    #[wasm_bindgen(getter, unchecked_return_type = "Adapter")]
    pub fn adapter(&self) -> JsValue {
        let gpu = self.renderer.gpu();
        let adapter = serde_json::json!({
            "name": gpu.name,
            "backend": if gpu.web_gpu { "webgpu" } else { "webgl" },
            "software": gpu.tier == Tier::Low,
            "tier": match gpu.tier {
                Tier::Low => "low",
                Tier::Mid => "mid",
                Tier::High => "high",
            },
        });
        js_sys::JSON::parse(&adapter.to_string()).unwrap_or_default()
    }

    #[wasm_bindgen(js_name = setSkin)]
    pub fn set_skin(&mut self, index: u32, rgba: &[u8]) -> Result<(), JsError> {
        let skin =
            skin::normalize(rgba).ok_or_else(|| JsError::new("skins must be 64x64 or 64x32"))?;
        self.renderer.set_skin(index + 1, &skin);
        Ok(())
    }

    pub fn toggle(&mut self) {
        if !self.playing && self.time >= self.duration {
            self.time = 0.0;
        }
        self.playing = !self.playing && self.session.is_some();
    }

    pub fn seek(&mut self, ms: f64) {
        self.time = ms.clamp(0.0, self.duration);
    }

    #[wasm_bindgen(js_name = setSpeed)]
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(SPEED_RANGE[0], SPEED_RANGE[1]);
    }

    #[wasm_bindgen(getter)]
    pub fn time(&self) -> f64 {
        self.time
    }

    #[wasm_bindgen(getter)]
    pub fn playing(&self) -> bool {
        self.playing
    }

    #[wasm_bindgen(getter, js_name = durationMs)]
    pub fn duration_ms(&self) -> f64 {
        self.duration
    }

    #[wasm_bindgen(getter, unchecked_return_type = "Camera")]
    pub fn camera(&self) -> String {
        self.controls.mode().name().to_owned()
    }

    #[wasm_bindgen(getter, js_name = autoCamera)]
    pub fn auto_camera(&self) -> bool {
        self.controls.auto()
    }

    #[wasm_bindgen(setter, js_name = autoCamera)]
    pub fn set_auto_camera(&mut self, on: bool) {
        match self.followed {
            Some(index) if on => self.watch(index, Mode::Follow),
            _ => self.controls.take_over(),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn followed(&self) -> Option<u32> {
        self.followed
    }

    #[wasm_bindgen(getter, js_name = flySpeed)]
    pub fn fly_speed(&self) -> f32 {
        self.controls.fly_speed()
    }

    #[wasm_bindgen(js_name = setCamera)]
    pub fn set_camera(&mut self, #[wasm_bindgen(unchecked_param_type = "Camera")] name: &str) {
        match Mode::ALL.into_iter().find(|mode| mode.name() == name) {
            Some(mode @ (Mode::Follow | Mode::Pov)) => self.watch(self.followed.unwrap_or(0), mode),
            Some(mode) => self.controls.set_mode(mode, None),
            None => {}
        }
    }

    #[wasm_bindgen(js_name = setFollowed)]
    pub fn set_followed(&mut self, index: u32) {
        self.followed = Some(index);
    }

    pub fn follow(&mut self, index: u32) {
        let mode = match self.controls.mode() {
            Mode::Pov => Mode::Pov,
            _ => Mode::Follow,
        };
        self.watch(index, mode);
    }

    #[wasm_bindgen(js_name = pickPlayer)]
    pub fn pick_player(&self, x: f32, y: f32, radius: f32) -> Option<u32> {
        let (session, assets) = (self.session.as_ref()?, self.assets.as_ref()?);
        let pointer = Vec2::new(x, y);
        let eye = self.controls.shown().position;
        let hidden = self.hidden();
        (0..session.people.len())
            .filter(|&index| Some(index) != hidden)
            .filter_map(|index| {
                let frame = session.player(index, self.time as f32, &assets.models)?;
                let near = PICK_HEIGHTS.iter().any(|&height| {
                    self.controls
                        .screen(frame.position + Vec3::Y * height)
                        .is_some_and(|point| point.distance(pointer) < radius)
                });
                near.then(|| (index, frame.position.distance_squared(eye)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index as u32)
    }

    pub fn presence(&self) -> Vec<u8> {
        self.session.as_ref().map_or_else(Vec::new, |session| {
            (0..session.people.len())
                .map(|index| u8::from(session.players.sample(index, self.time as f32).is_some()))
                .collect()
        })
    }

    #[wasm_bindgen(getter, js_name = pendingSections)]
    pub fn pending_sections(&self) -> usize {
        self.session
            .as_ref()
            .map_or(0, |session| session.pending.len())
    }

    #[wasm_bindgen(getter, js_name = soundNames)]
    pub fn sound_names(&self) -> Vec<String> {
        self.session
            .as_ref()
            .map_or_else(Vec::new, |session| session.sounds.names.clone())
    }

    #[wasm_bindgen(getter)]
    pub fn heard(&self) -> Vec<f32> {
        self.heard.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn listener(&self) -> Vec<f32> {
        let Listener {
            position,
            forward,
            up,
            ..
        } = self.controls.listener();
        [position, forward, up]
            .iter()
            .flat_map(|axis| axis.to_array())
            .collect()
    }

    #[wasm_bindgen(js_name = setInsets)]
    pub fn set_insets(&mut self, right: f32, bottom: f32) {
        self.controls.set_insets(Vec2::new(right, bottom));
    }

    #[wasm_bindgen(js_name = setReducedMotion)]
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.controls.set_reduced_motion(reduced);
    }

    pub fn resize(&mut self, width: f64, height: f64, pixel_ratio: f64) {
        let [pixels_wide, pixels_high] =
            [width, height].map(|size| (size * pixel_ratio).round().max(1.0) as u32);
        self.canvas.set_width(pixels_wide);
        self.canvas.set_height(pixels_high);
        self.renderer.resize(pixels_wide, pixels_high);
        self.controls.resize(width as f32, height as f32);
    }

    pub fn input(&mut self, event: &Event) {
        if let Some(key) = event.dyn_ref::<KeyboardEvent>() {
            self.controls.key(key.code(), event.type_() == "keydown");
        } else if let Some(wheel) = event.dyn_ref::<WheelEvent>() {
            self.on_wheel(event, wheel);
        } else if let Some(touch) = event
            .dyn_ref::<PointerEvent>()
            .filter(|pointer| pointer.pointer_type() != "mouse")
        {
            self.on_touch(event, touch);
        } else if let Some(mouse) = event.dyn_ref::<MouseEvent>() {
            self.on_mouse(event, mouse);
        } else if event.type_() == "blur" {
            self.controls.release();
        }
    }

    pub fn frame(&mut self, now: f64) {
        let dt = self
            .last_frame
            .map_or(0.0, |last| (now - last).clamp(0.0, MAX_FRAME_MS));
        self.last_frame = Some(now);
        let from = self.time;
        let audible = self.playing && self.speed <= MAX_AUDIBLE_SPEED;
        if self.playing {
            self.time = (self.time + dt * self.speed).min(self.duration);
            self.playing = self.time < self.duration;
        }
        let subject = self.subject();
        let (session, assets) = (&self.session, &self.assets);
        let camera = self
            .controls
            .update(dt as f32 / 1000.0, subject.as_ref(), |block| {
                session
                    .as_ref()
                    .zip(assets.as_ref())
                    .is_some_and(|(session, assets)| solid(session, assets, block))
            });
        let hidden = self.hidden();
        self.scene.clear();
        self.scene.time = self.time as f32;
        self.scene.focus = subject.filter(|_| hidden.is_none()).map(|frame| {
            (
                frame.position + Vec3::Y * REVEAL_HEIGHT,
                self.controls.veil(),
            )
        });
        self.heard.clear();
        if let Some(session) = &mut self.session {
            session.world.seek(self.time as u32);
            session.refresh();
            if let Some(assets) = &self.assets {
                let daylight = session.daylight(self.time as u32);
                self.renderer.set_daylight(daylight);
                let [fog_start, fog_end] = self.controls.fog();
                self.renderer.set_fog(fog_start, fog_end);
                session.mesh(&assets.models, &mut self.renderer, camera.position);
                session.draw(
                    &mut self.scene,
                    assets,
                    &camera,
                    daylight,
                    self.time as f32,
                    hidden,
                );
                if audible {
                    let listener = self.controls.listener();
                    let span = [from as f32, self.time as f32];
                    session.hear(span, &listener, &assets.models, &mut self.heard);
                }
            }
        }
        self.renderer.render(&camera, &self.scene);
    }

    fn watch(&mut self, index: u32, mode: Mode) {
        self.followed = Some(index);
        let subject = self.subject();
        self.controls.set_mode(mode, subject.as_ref());
    }

    fn subject(&self) -> Option<PlayerFrame> {
        let index = self.followed? as usize;
        let models = &self.assets.as_ref()?.models;
        self.session
            .as_ref()?
            .player(index, self.time as f32, models)
    }

    fn hidden(&self) -> Option<usize> {
        let index = self.followed.filter(|_| self.controls.inside_subject())?;
        Some(index as usize)
    }
}

impl Viewer {
    fn on_wheel(&mut self, event: &Event, wheel: &WheelEvent) {
        let reach = self.controls.fog()[1];
        let pick = block_picker(self.session.as_ref(), self.assets.as_ref(), reach);
        let scale = match wheel.delta_mode() {
            WheelEvent::DOM_DELTA_LINE => 16.0,
            WheelEvent::DOM_DELTA_PAGE => 400.0,
            _ => 1.0,
        };
        let at = pair(event, "offsetX", "offsetY");
        let ray = self.controls.ray(at.x, at.y);
        let delta = (wheel.delta_y() * scale) as f32;
        self.controls
            .wheel(delta, wheel.ctrl_key(), &ray, pick(&ray));
    }

    fn on_touch(&mut self, event: &Event, touch: &PointerEvent) {
        let reach = self.controls.fog()[1];
        let pick = block_picker(self.session.as_ref(), self.assets.as_ref(), reach);
        let id = touch.pointer_id();
        let at = pair(event, "offsetX", "offsetY");
        match event.type_().as_str() {
            "pointerdown" => self.controls.touch_down(id, at, pick),
            "pointermove" => self.controls.touch_move(id, at),
            "pointerup" | "pointercancel" => self.controls.touch_up(id),
            _ => {}
        }
    }

    fn on_mouse(&mut self, event: &Event, mouse: &MouseEvent) {
        let reach = self.controls.fog()[1];
        let pick = block_picker(self.session.as_ref(), self.assets.as_ref(), reach);
        let at = pair(event, "offsetX", "offsetY");
        let ray = self.controls.ray(at.x, at.y);
        match event.type_().as_str() {
            "pointerdown" => {
                let pan = mouse.button() != 0
                    || mouse.shift_key()
                    || mouse.ctrl_key()
                    || mouse.meta_key();
                self.controls.press(at, pan, pick);
            }
            "pointermove" => {
                let locked = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.pointer_lock_element())
                    .is_some();
                if mouse.buttons() != 0 || locked {
                    let movement = pair(event, "movementX", "movementY");
                    self.controls.drag(movement.x, movement.y, &ray);
                }
            }
            "pointerup" | "pointercancel" => self.controls.lift(),
            "dblclick" => {
                if let Some(block) = pick(&ray) {
                    self.controls.focus(block);
                }
            }
            _ => {}
        }
    }
}

impl Session {
    fn player(&self, index: usize, t: f32, models: &BlockModels) -> Option<PlayerFrame> {
        let frame = self.players.sample(index, t)?;
        let seat = self
            .players
            .vehicle(index, t)
            .and_then(|vehicle| self.objects.seat(vehicle, t, &self.world, models));
        Some(seat.map_or(frame, |seat| frame.seated(seat)))
    }

    fn refresh(&mut self) {
        let dirty: Vec<_> = self.world.drain_dirty().collect();
        for &section in &dirty {
            self.chests.scan(&self.world, section);
        }
        self.pending.extend(dirty);
    }

    fn daylight(&self, t: u32) -> Daylight {
        let passed = self.daytime.partition_point(|&(at, _)| at <= t);
        self.daytime[..passed]
            .last()
            .or(self.daytime.first())
            .map_or_else(Daylight::default, |&(_, tick)| Daylight::at(tick))
    }

    fn mesh(&mut self, models: &BlockModels, renderer: &mut Renderer, eye: Vec3) {
        let distance = |&section: &[i32; 3]| {
            (IVec3::from(section).as_vec3() * 16.0 + 8.0).distance_squared(eye)
        };
        let stale: Vec<_> = self.world.drain_stale_light().collect();
        if !stale.is_empty() {
            let relit = self.lighting.update(&self.world, stale);
            self.pending.extend(relit);
        }
        let mut nearest: Vec<_> = self.pending.iter().copied().collect();
        nearest.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        let deadline = now() + MESH_BUDGET_MS;
        for section in nearest {
            self.lighting.prepare(&self.world, section);
            let mesh = mesh_section(&self.world, &self.lighting, models, section);
            renderer.upload_section(section, &mesh);
            self.pending.remove(&section);
            if now() > deadline {
                break;
            }
        }
    }

    fn draw(
        &mut self,
        scene: &mut Scene,
        assets: &Assets,
        camera: &Camera,
        daylight: Daylight,
        t: f32,
        hidden: Option<usize>,
    ) {
        let mut draw = Draw {
            scene,
            assets,
            camera,
            items: &self.items,
            skulls: &self.skulls,
            t,
            lighting: &self.lighting,
            daylight,
            light: WHITE,
        };
        for (index, person) in self.people.iter().enumerate() {
            if Some(index) != hidden
                && let Some(frame) = self.player(index, t, &assets.models)
            {
                draw.light_at(frame.position + Vec3::Y * EYE_HEIGHT);
                players::draw(&mut draw, &frame, person);
            }
        }
        self.objects.draw(&mut draw, &self.world);
        self.chests.draw(&mut draw, &self.world);
        draw.light = WHITE;
        self.cracks.draw(&mut draw, &self.world);
        let particles = self.particles.update(t, &self.world, &assets.models);
        let swirls = self.players.swirls(t, hidden);
        let trails = self.objects.trails(t, &self.world, &assets.models);
        effects::particles(
            &mut draw,
            particles.iter().copied().chain(swirls).chain(trails),
        );
    }

    fn hear(
        &self,
        [after, until]: [f32; 2],
        listener: &Listener,
        models: &BlockModels,
        out: &mut Vec<f32>,
    ) {
        let audible = self
            .sounds
            .between(after, until)
            .iter()
            .filter_map(|sound| {
                let at = match sound.origin {
                    Origin::Point(at) => at,
                    Origin::Player(index) => self.player(index, until, models)?.position,
                };
                let reach = HEARING * sound.volume.max(1.0);
                let distance = at.distance(listener.position);
                let name = sound
                    .far
                    .filter(|_| distance >= FAR_SOUND_DISTANCE)
                    .unwrap_or(sound.name);
                (distance <= reach).then_some([
                    name as f32,
                    at.x,
                    at.y,
                    at.z,
                    sound.volume,
                    sound.pitch,
                    reach,
                ])
            });
        out.extend(audible.take(MAX_SOUNDS_PER_FRAME).flatten());
    }
}

fn allocate_skins(
    replay: &Replay,
    renderer: &mut Renderer,
    assets: &Assets,
) -> (Vec<Person>, Vec<i32>, Vec<String>) {
    let mut skins: Vec<String> = Vec::new();
    let mut layer = |hash: Option<&String>| {
        hash.map_or(0, |hash| {
            let known = skins.iter().position(|skin| skin == hash);
            known.unwrap_or_else(|| {
                skins.push(hash.clone());
                skins.len() - 1
            }) as i32
                + 1
        })
    };
    let mut people: Vec<Person> = replay
        .players
        .iter()
        .map(|player| Person {
            name: player.username.clone(),
            color: player.team_color,
            slim: player.slim,
            skin: layer(player.skin_hash.as_ref()),
        })
        .collect();
    let mut skulls: Vec<i32> = replay
        .items
        .iter()
        .map(|item| layer(item.skin.as_ref()))
        .collect();
    let mut unskinned = skins.len() as i32;
    for person in people.iter_mut().filter(|person| person.skin == 0) {
        unskinned += 1;
        person.skin = unskinned;
    }
    let layers = renderer.set_skin_count(unskinned as u32 + 1) as i32;
    for skin in people
        .iter_mut()
        .map(|person| &mut person.skin)
        .chain(&mut skulls)
    {
        *skin = if *skin < layers { *skin } else { 0 };
    }
    for layer in 0..layers {
        renderer.set_skin(layer as u32, &assets.steve);
    }
    for person in people.iter().rev().filter(|person| person.skin > 0) {
        renderer.set_skin(person.skin as u32, &skin::fallback(&person.name));
    }
    (people, skulls, skins)
}

fn pair(event: &Event, x: &str, y: &str) -> Vec2 {
    let read = |name: &str| {
        js_sys::Reflect::get(event, &name.into())
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or_default() as f32
    };
    Vec2::new(read(x), read(y))
}

fn block_picker(
    session: Option<&Session>,
    assets: Option<&Assets>,
    reach: f32,
) -> impl Fn(&Ray) -> Option<Vec3> {
    move |ray: &Ray| pick_block(session?, assets?, ray, reach)
}

fn pick_block(session: &Session, assets: &Assets, ray: &Ray, reach: f32) -> Option<Vec3> {
    let steps = (reach / PICK_STEP) as u32;
    (0..steps)
        .map(|step| ray.at(step as f32 * PICK_STEP))
        .find(|point| solid(session, assets, point.floor().as_ivec3()))
}

fn solid(session: &Session, assets: &Assets, block: IVec3) -> bool {
    let state = session.world.block(block.into());
    assets.models.block(state).flags.has(Flags::SOLID)
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}
