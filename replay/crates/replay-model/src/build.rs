use std::{collections::HashMap, f32::consts::TAU};

use replay_file::Recording;
use replay_protocol::{
    Clientbound, Metadata, MetadataValue, Slot, legacy_color, plain_text, segments,
};

use crate::{
    Effect, EffectKind, EntityState, Equipment, Event, EventKind, Info, Item, Look, MoveKind,
    Moves, Object, ObjectKind, Player, Replay,
    clock::{Clock, clock_at},
    item::Items,
    roster::Roster,
    scoreboard::{Scoreboard, Sidebar},
    strip,
    world::WorldLog,
};

const EVENT_DEDUP_MS: u32 = 45;
const DAY_TICKS: u64 = 24000;
const WHITE: u32 = 0xFFFFFF;
const PLAYER_HEIGHT: f32 = 1.8;
const FIREWORK_BURST: i8 = 17;
const GOLEM_ATTACK: i8 = 4;
const ON_FIRE: u8 = 0x01;
const STABLE_WINDOW_MS: u32 = 5000;
const SAMPLE_MS: u32 = 50;
const TELEPORT_DISTANCE: f32 = 16.0;
const TEAM_COLORS: [(char, &[u32]); 8] = [
    ('c', &[0xFF0000, 0xFF5555, 0xAA0000]),
    ('9', &[0x0000FF, 0x5555FF, 0x0000AA]),
    ('a', &[0x48CC18, 0x55FF55, 0x00AA00, 0x008000]),
    ('e', &[0xFFFF00, 0xFFFF55]),
    ('b', &[0x00FFFF, 0x55FFFF, 0x00AAAA]),
    ('f', &[0xFFFFFF]),
    ('d', &[0xEF83A4, 0xFF55FF, 0xAA00AA, 0x800080]),
    ('8', &[0x808080, 0x555555, 0xAAAAAA]),
];

pub fn build(recording: &Recording) -> Replay {
    let mut builder = Builder::new(Clock::new(recording));
    let mut raw = 0;
    for packet in recording.packets() {
        raw = packet.time.max(raw);
        if raw > builder.clock.finish {
            break;
        }
        if !builder.started && raw >= builder.clock.start {
            builder.begin();
        }
        builder.t = builder.clock.elapsed(raw);
        if let Some(decoded) = replay_protocol::decode(packet.id, packet.body) {
            builder.handle(raw, decoded);
        }
    }
    builder.finish(recording)
}

struct Builder {
    clock: Clock,
    started: bool,
    t: u32,
    recorder: Option<i32>,
    dimension: i32,
    entities: HashMap<i32, Entity>,
    roster: Roster,
    objects: Vec<Object>,
    items: Items,
    events: Vec<Event>,
    last_event: HashMap<EventKind, u32>,
    effects: Vec<Effect>,
    daytime: Vec<(u32, u16)>,
    world: WorldLog,
    scoreboard: Scoreboard,
    sidebar: Sidebar,
}

struct Entity {
    body: Body,
    pos: [f64; 3],
    yaw: i8,
    pitch: i8,
    head_yaw: i8,
    equipment: Equipment,
    state: EntityState,
    vehicle: Option<usize>,
    name: String,
    name_visible: bool,
}

#[derive(Clone, Copy)]
enum Body {
    Player(u128),
    Object(usize),
}

impl Builder {
    fn new(clock: Clock) -> Builder {
        Builder {
            clock,
            started: false,
            t: 0,
            recorder: None,
            dimension: 0,
            entities: HashMap::new(),
            roster: Roster::default(),
            objects: Vec::new(),
            items: Items::default(),
            events: Vec::new(),
            last_event: HashMap::new(),
            effects: Vec::new(),
            daytime: Vec::new(),
            world: WorldLog::default(),
            scoreboard: Scoreboard::default(),
            sidebar: Sidebar::default(),
        }
    }

    fn begin(&mut self) {
        self.started = true;
        self.world.begin();
        self.sidebar.read(&self.scoreboard);
        let entities = std::mem::take(&mut self.entities);
        let mut alive: Vec<_> = entities.iter().collect();
        alive.sort_by_key(|&(&id, _)| id);
        for (_, entity) in alive {
            self.appear(entity);
        }
        self.entities = entities;
    }

    fn handle(&mut self, raw: u32, packet: Clientbound) {
        let t = self.t;
        match packet {
            Clientbound::JoinGame {
                entity, dimension, ..
            } => {
                self.recorder = Some(entity);
                self.dimension = dimension.into();
            }
            Clientbound::Respawn { dimension, .. } if dimension != self.dimension => {
                self.change_dimension(dimension)
            }
            Clientbound::Chat { json, position: 2 } => self.clock.read_action_bar(raw, &json),
            Clientbound::TimeUpdate { time_of_day } => self.record_daytime(time_of_day),
            Clientbound::Chat { json, .. } => self.chat(&json),
            Clientbound::SpawnPlayer {
                entity,
                uuid,
                pos,
                yaw,
                pitch,
                metadata,
                ..
            } if self.recorder != Some(entity) => {
                self.spawn_player(entity, uuid, pos, [yaw, pitch], &metadata)
            }
            Clientbound::SpawnObject {
                entity,
                kind,
                pos,
                pitch,
                yaw,
                data,
                velocity,
            } => self.spawn_object(entity, kind, data, velocity, pos, [yaw, pitch]),
            Clientbound::SpawnMob {
                entity,
                kind,
                pos,
                yaw,
                pitch,
                head_yaw,
                metadata,
                ..
            } => self.spawn_mob(entity, kind, pos, [yaw, pitch, head_yaw], &metadata),
            Clientbound::DestroyEntities(ids) => {
                for id in ids {
                    self.despawn(id);
                }
            }
            Clientbound::EntityVelocity { entity, velocity } => {
                if let Some(object) = self.object(entity) {
                    object.record_velocity(t, velocity);
                }
            }
            Clientbound::EntityRelativeMove { entity, delta, .. } => {
                self.moved(entity, MoveKind::Relative, |entity| {
                    entity.shift(delta);
                })
            }
            Clientbound::EntityLook {
                entity, yaw, pitch, ..
            } => self.moved(entity, MoveKind::Look, |entity| {
                entity.turn(yaw, pitch);
            }),
            Clientbound::EntityLookAndRelativeMove {
                entity,
                delta,
                yaw,
                pitch,
                ..
            } => self.moved(entity, MoveKind::LookAndRelative, |entity| {
                entity.shift(delta);
                entity.turn(yaw, pitch);
            }),
            Clientbound::EntityTeleport {
                entity,
                pos,
                yaw,
                pitch,
                ..
            } => self.moved(entity, MoveKind::Teleport, |entity| {
                entity.pos = pos;
                entity.turn(yaw, pitch);
            }),
            Clientbound::EntityHeadLook { entity, head_yaw } => {
                self.moved(entity, MoveKind::HeadLook, |entity| {
                    entity.head_yaw = head_yaw
                })
            }
            Clientbound::EntityEquipment { entity, slot, item } => {
                self.equip(entity, slot, item.as_ref())
            }
            Clientbound::EntityMetadata { entity, metadata } => {
                self.update_metadata(entity, &metadata)
            }
            Clientbound::AttachEntity {
                entity,
                vehicle,
                leash: false,
            } => self.attach(entity, vehicle),
            Clientbound::Animation { entity, animation } => self.animate(entity, animation),
            Clientbound::EntityStatus { entity, status } => self.entity_status(entity, status),
            Clientbound::CollectItem {
                collected,
                collector,
            } => self.collect(collected, collector),
            Clientbound::BlockBreakAnimation {
                breaker,
                pos,
                stage,
            } => self.break_block(breaker, pos, stage),
            packet @ (Clientbound::Particle { .. }
            | Clientbound::NamedSound { .. }
            | Clientbound::Effect { .. }
            | Clientbound::BlockAction { .. }) => self.record_effect(packet),
            Clientbound::ChunkData(column) => self.world.load(column, t),
            Clientbound::MapChunkBulk(columns) => {
                for column in columns {
                    self.world.load(column, t);
                }
            }
            Clientbound::BlockChange { pos, state } => self.world.set(pos, state, t),
            Clientbound::MultiBlockChange(changes) => {
                for (pos, state) in changes {
                    self.world.set(pos, state, t);
                }
            }
            Clientbound::Explosion {
                pos,
                radius,
                blocks,
                ..
            } => self.explode(pos, radius, blocks),
            Clientbound::PlayerListItem(entries) => {
                for (uuid, action) in entries {
                    self.roster.tab(uuid, action);
                }
            }
            Clientbound::ScoreboardObjective { name, removed } => {
                self.scoreboard.objective(name, removed)
            }
            Clientbound::UpdateScore {
                entry,
                objective,
                value,
            } => self.scoreboard.score(entry, &objective, value),
            Clientbound::DisplayScoreboard {
                position,
                objective,
            } => self.scoreboard.display(position, objective),
            Clientbound::Teams { name, action } => self.scoreboard.team(name, action),
            _ => {}
        }
    }

    fn change_dimension(&mut self, dimension: i32) {
        self.dimension = dimension;
        self.world.clear(self.t);
        for entity in std::mem::take(&mut self.entities).into_values() {
            self.vanish(&entity);
        }
    }

    fn record_daytime(&mut self, time_of_day: i64) {
        let tick = (time_of_day.unsigned_abs() % DAY_TICKS) as u16;
        if self.daytime.last().is_none_or(|&(_, last)| last != tick) {
            self.daytime.push((self.t, tick));
        }
    }

    fn chat(&mut self, json: &str) {
        let text = plain_text(json);
        let text = text.trim();
        if is_replay_chat(text) {
            self.event(EventKind::Chat {
                text: text.to_owned(),
                segments: segments(json),
            });
        }
    }

    fn spawn_player(
        &mut self,
        id: i32,
        uuid: u128,
        pos: [f64; 3],
        [yaw, pitch]: [i8; 2],
        metadata: &Metadata,
    ) {
        let mut entity = Entity::new(Body::Player(uuid), pos, yaw, pitch, yaw);
        entity.apply(metadata);
        self.spawn(id, entity);
    }

    fn spawn_object(
        &mut self,
        id: i32,
        kind: u8,
        data: i32,
        velocity: Option<[i16; 3]>,
        pos: [f64; 3],
        [yaw, pitch]: [i8; 2],
    ) {
        match ObjectKind::object(kind) {
            Some(kind) => {
                let mut object = Object::spawn(kind, data);
                if let Some(velocity) = velocity {
                    object.record_velocity(self.t, velocity);
                }
                let body = self.track(object);
                self.spawn(id, Entity::new(body, pos, yaw, pitch, yaw));
            }
            None => self.despawn(id),
        }
    }

    fn spawn_mob(
        &mut self,
        id: i32,
        kind: u8,
        pos: [f64; 3],
        [yaw, pitch, head_yaw]: [i8; 3],
        metadata: &Metadata,
    ) {
        match ObjectKind::mob(kind) {
            Some(kind) => {
                let mut object = Object::spawn(kind, 0);
                object.apply(metadata, &mut self.items);
                let mut entity = Entity::new(self.track(object), pos, yaw, pitch, head_yaw);
                entity.apply(metadata);
                self.spawn(id, entity);
            }
            None => self.despawn(id),
        }
    }

    fn equip(&mut self, id: i32, slot: i16, item: Option<&Slot>) {
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        let Some(worn) = entity.equipment.slot(slot) else {
            return;
        };
        *worn = self.items.intern(item);
        match entity.body {
            Body::Player(uuid) if self.started => self
                .roster
                .player(uuid)
                .equipment
                .push((self.t, entity.equipment)),
            Body::Player(_) => {}
            Body::Object(index) => {
                let object = &mut self.objects[index];
                object.helmet = entity.equipment.helmet;
                object.hand = entity.equipment.held;
            }
        }
    }

    fn update_metadata(&mut self, id: i32, metadata: &Metadata) {
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        entity.apply(metadata);
        match entity.body {
            Body::Player(uuid) if self.started => {
                self.roster.player(uuid).states.push((self.t, entity.state))
            }
            Body::Player(_) => {}
            Body::Object(index) => {
                let object = &mut self.objects[index];
                object.apply(metadata, &mut self.items);
                if self.started {
                    object.record_name(self.t, entity.shown_name());
                    object.record_burning(self.t, entity.on_fire());
                }
            }
        }
    }

    fn attach(&mut self, id: i32, vehicle: i32) {
        let vehicle = self.entities.get(&vehicle).and_then(Entity::object);
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        entity.vehicle = vehicle;
        if self.started {
            match entity.body {
                Body::Player(uuid) => self.roster.player(uuid).riding.push((self.t, vehicle)),
                Body::Object(index) => self.objects[index].riding.push((self.t, vehicle)),
            }
        }
    }

    fn animate(&mut self, id: i32, animation: u8) {
        let Some(entity) = self.entities.get(&id) else {
            return;
        };
        let pos = entity.position();
        let height = match entity.body {
            Body::Player(_) => PLAYER_HEIGHT,
            Body::Object(index) => self.objects[index].kind.height(),
        };
        let player = self.player_of(id);
        match (animation, player) {
            (0, Some(player)) => self.event(EventKind::Swing(player)),
            (1, Some(player)) => self.event(EventKind::Hurt(player)),
            (4 | 5, _) if self.started => self.effects.push(Effect {
                t: self.t,
                kind: EffectKind::Crit {
                    magic: animation == 5,
                    player,
                    pos,
                    height,
                },
            }),
            _ => {}
        }
    }

    fn entity_status(&mut self, id: i32, status: i8) {
        let t = self.t;
        if let Some(player) = self.player_of(id) {
            match status {
                2 => self.event(EventKind::Hurt(player)),
                3 => self.event(EventKind::Dead(player)),
                _ => {}
            }
        } else if let Some(object) = self.object(id) {
            match (object.kind, status) {
                (ObjectKind::Firework, FIREWORK_BURST) => object.burst = Some(t),
                (ObjectKind::IronGolem, GOLEM_ATTACK) => object.attacks.push(t),
                _ => {}
            }
        }
    }

    fn collect(&mut self, collected: i32, collector: i32) {
        if let Some(player) = self.player_of(collector) {
            self.event(EventKind::Collect { player, collected });
        }
    }

    fn break_block(&mut self, breaker: i32, pos: [i32; 3], stage: i8) {
        let player = self.player_of(breaker);
        self.event(EventKind::BlockBreak {
            breaker,
            player,
            pos,
            stage: if (0..=9).contains(&stage) { stage } else { -1 },
        });
    }

    fn record_effect(&mut self, packet: Clientbound) {
        if let Some(kind) = EffectKind::of(packet)
            && (self.started || matches!(kind, EffectKind::Lid { .. }))
        {
            self.effects.push(Effect { t: self.t, kind });
        }
    }

    fn explode(&mut self, pos: [f32; 3], radius: f32, blocks: Vec<[i32; 3]>) {
        for &block in &blocks {
            self.world.set(block, 0, self.t);
        }
        if self.started {
            self.effects.push(Effect {
                t: self.t,
                kind: EffectKind::Explosion {
                    pos,
                    radius,
                    blocks,
                },
            });
        }
    }

    fn track(&mut self, object: Object) -> Body {
        self.objects.push(object);
        Body::Object(self.objects.len() - 1)
    }

    fn spawn(&mut self, id: i32, entity: Entity) {
        self.despawn(id);
        if self.started {
            self.appear(&entity);
        }
        self.entities.insert(id, entity);
    }

    fn despawn(&mut self, id: i32) {
        if let Some(entity) = self.entities.remove(&id) {
            self.vanish(&entity);
        }
    }

    fn appear(&mut self, entity: &Entity) {
        match entity.body {
            Body::Player(uuid) => self.roster.player(uuid).appear(self.t, entity),
            Body::Object(index) => {
                let object = &mut self.objects[index];
                object.record_path(self.t, entity.position(), entity.look());
                object.record_name(self.t, entity.shown_name());
                object.record_burning(self.t, entity.on_fire());
                if entity.vehicle.is_some() {
                    object.riding.push((self.t, entity.vehicle));
                }
            }
        }
    }

    fn vanish(&mut self, entity: &Entity) {
        if self.started {
            match entity.body {
                Body::Player(uuid) => self.roster.player(uuid).presence.push((self.t, false)),
                Body::Object(index) => self.objects[index].end = Some(self.t),
            }
        }
    }

    fn moved(&mut self, id: i32, kind: MoveKind, change: impl FnOnce(&mut Entity)) {
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        change(entity);
        if self.started {
            match entity.body {
                Body::Player(uuid) => self.roster.player(uuid).moves.push(self.t, entity, kind),
                Body::Object(index) => {
                    self.objects[index].record_path(self.t, entity.position(), entity.look())
                }
            }
        }
    }

    fn player_of(&mut self, id: i32) -> Option<usize> {
        let Body::Player(uuid) = self.entities.get(&id)?.body else {
            return None;
        };
        self.started.then(|| self.roster.ensure(uuid))
    }

    fn object(&mut self, id: i32) -> Option<&mut Object> {
        let index = self.entities.get(&id)?.object()?;
        self.started.then(|| &mut self.objects[index])
    }

    fn event(&mut self, kind: EventKind) {
        let repeated = self
            .last_event
            .get(&kind)
            .is_some_and(|&last| self.t - last < EVENT_DEDUP_MS);
        if self.started && !repeated {
            self.last_event.insert(kind.clone(), self.t);
            self.events.push(Event { t: self.t, kind });
        }
    }

    fn finish(mut self, recording: &Recording) -> Replay {
        self.sidebar.read(&self.scoreboard);
        let mut players = self.roster.players;
        for player in &mut players {
            let recorded = self
                .scoreboard
                .team_color(&player.username)
                .filter(|&color| color != WHITE)
                .or_else(|| dominant_leather(player, &self.items.list, self.t))
                .unwrap_or(WHITE);
            player.team_color = TEAM_COLORS
                .iter()
                .find(|(_, colors)| colors.contains(&recorded))
                .and_then(|&(code, _)| legacy_color(code))
                .unwrap_or(recorded);
            if let Some(owner) = player.skin_owner.clone()
                && player.username.len() >= 14
                && owner.len() > player.username.len()
                && owner.starts_with(&player.username)
            {
                player.username = owner;
            }
        }
        let (kept, mut objects): (Vec<_>, Vec<_>) = self
            .objects
            .into_iter()
            .enumerate()
            .filter(|(_, object)| object.shown())
            .unzip();
        let ridings = objects
            .iter_mut()
            .map(|object| &mut object.riding)
            .chain(players.iter_mut().map(|player| &mut player.riding));
        for (_, vehicle) in ridings.flatten() {
            *vehicle = vehicle.and_then(|vehicle| kept.binary_search(&vehicle).ok());
        }
        let metadata = recording.metadata();
        Replay {
            info: Info {
                duration: self.clock.duration,
                start: self.clock.start,
                stable_start: stable_start(&players),
                server_name: metadata.server_name.clone(),
                game: self.sidebar.game.or(metadata.game.clone()),
                mode: self.sidebar.mode.or(metadata.mode.clone()),
                map: self.sidebar.map.or(metadata.map.clone()),
                date: self.sidebar.date,
                time: self.sidebar.time,
            },
            players,
            items: self.items.list,
            objects,
            events: self.events,
            effects: self.effects,
            daytime: self.daytime,
            world: self.world.finish(),
        }
    }
}

impl Entity {
    fn new(body: Body, pos: [f64; 3], yaw: i8, pitch: i8, head_yaw: i8) -> Entity {
        Entity {
            body,
            pos,
            yaw,
            pitch,
            head_yaw,
            equipment: Equipment::default(),
            state: EntityState::default(),
            vehicle: None,
            name: String::new(),
            name_visible: false,
        }
    }

    fn apply(&mut self, metadata: &Metadata) {
        for (index, value) in metadata {
            match (index, value) {
                (0, MetadataValue::Byte(flags)) => self.state.flags = *flags,
                (2, MetadataValue::String(name)) => self.name.clone_from(name),
                (3, MetadataValue::Byte(visible)) => self.name_visible = *visible != 0,
                (6, MetadataValue::Float(health)) => self.state.health = Some(*health),
                (7, MetadataValue::Int(color)) => self.state.potion_color = *color,
                _ => {}
            }
        }
    }

    fn object(&self) -> Option<usize> {
        match self.body {
            Body::Object(index) => Some(index),
            Body::Player(_) => None,
        }
    }

    fn position(&self) -> [f32; 3] {
        self.pos.map(|axis| axis as f32)
    }

    fn look(&self) -> Look {
        let angle = |byte: i8| f32::from(byte) * TAU / 256.0;
        Look {
            yaw: angle(self.yaw),
            pitch: angle(self.pitch),
            head_yaw: angle(self.head_yaw),
        }
    }

    fn shown_name(&self) -> Option<String> {
        let shown = self.name_visible && !strip(&self.name).trim().is_empty();
        shown.then(|| self.name.clone())
    }

    fn on_fire(&self) -> bool {
        self.state.flags & ON_FIRE != 0
    }

    fn shift(&mut self, delta: [f64; 3]) {
        for (axis, delta) in self.pos.iter_mut().zip(delta) {
            *axis += delta;
        }
    }

    fn turn(&mut self, yaw: i8, pitch: i8) {
        self.yaw = yaw;
        self.pitch = pitch;
    }
}

impl Equipment {
    fn slot(&mut self, slot: i16) -> Option<&mut Option<u32>> {
        Some(match slot {
            0 => &mut self.held,
            1 => &mut self.boots,
            2 => &mut self.leggings,
            3 => &mut self.chestplate,
            4 => &mut self.helmet,
            _ => return None,
        })
    }

    fn armor(&self) -> [Option<u32>; 4] {
        [self.boots, self.leggings, self.chestplate, self.helmet]
    }
}

impl Player {
    fn appear(&mut self, t: u32, entity: &Entity) {
        self.presence.push((t, true));
        self.moves.push(t, entity, MoveKind::Spawn);
        self.equipment.push((t, entity.equipment));
        self.states.push((t, entity.state));
        if self.riding.last().and_then(|&(_, vehicle)| vehicle) != entity.vehicle {
            self.riding.push((t, entity.vehicle));
        }
    }
}

impl Moves {
    fn push(&mut self, t: u32, entity: &Entity, kind: MoveKind) {
        let look = entity.look();
        self.t.push(t);
        self.pos.push(entity.position());
        self.yaw.push(look.yaw);
        self.pitch.push(look.pitch);
        self.head_yaw.push(look.head_yaw);
        self.kind.push(kind);
    }
}

fn is_replay_chat(text: &str) -> bool {
    let banner = text
        .chars()
        .all(|character| character.is_whitespace() || "■□▌▐─═".contains(character));
    let finished = text
        .strip_prefix("Finished")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .and_then(|rest| clock_at(rest.trim_start()))
        .is_some();
    !(banner
        || finished
        || text.contains("Playing")
        || text.contains("Paused")
        || text == "You went too far away from the map!")
}

fn stable_start(players: &[Player]) -> u32 {
    let position = |player: &Player, t: u32| {
        let presence = &player.presence;
        let present = presence[..presence.partition_point(|&(at, _)| at <= t)]
            .last()
            .is_some_and(|&(_, present)| present);
        let moves = &player.moves;
        let latest = moves.t.partition_point(|&at| at <= t).checked_sub(1)?;
        present.then_some(moves.pos[latest])
    };
    (SAMPLE_MS..STABLE_WINDOW_MS)
        .step_by(SAMPLE_MS as usize)
        .find(|&t| {
            let shown: Vec<_> = players
                .iter()
                .filter_map(|player| Some((position(player, t)?, position(player, t - SAMPLE_MS))))
                .collect();
            let teleported = shown
                .iter()
                .filter(|(now, before)| {
                    before.is_some_and(|before| {
                        let squared: f32 =
                            (0..3).map(|axis| (now[axis] - before[axis]).powi(2)).sum();
                        squared > TELEPORT_DISTANCE.powi(2)
                    })
                })
                .count();
            teleported as f32 >= (shown.len() as f32 / 2.0).max(2.0)
        })
        .unwrap_or(0)
}

fn dominant_leather(player: &Player, items: &[Item], end: u32) -> Option<u32> {
    let mut worn: Vec<(u32, u32)> = Vec::new();
    let until = player
        .equipment
        .iter()
        .skip(1)
        .map(|&(t, _)| t)
        .chain([end]);
    for (&(since, equipment), until) in player.equipment.iter().zip(until) {
        let armor = equipment
            .armor()
            .into_iter()
            .flatten()
            .map(|item| &items[item as usize]);
        for color in armor.filter(|item| item.dyed).filter_map(|item| item.color) {
            match worn.iter_mut().find(|(worn_color, _)| *worn_color == color) {
                Some((_, time)) => *time += until - since,
                None => worn.push((color, until - since)),
            }
        }
    }
    worn.iter()
        .rev()
        .max_by_key(|&&(_, time)| time)
        .map(|&(color, _)| color)
}
