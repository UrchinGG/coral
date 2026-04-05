use serde_json::Value;

use super::bedwars::GuildInfo;
use super::player::{calculate_network_level, extract_rank_prefix};


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivisionTrack {
    Default,
    Half,
    Overall,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DuelsView {
    Overall,
    Category(&'static str),
}


impl DuelsView {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Overall => "overall",
            Self::Category(id) => id,
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        if value == "overall" {
            return Some(Self::Overall);
        }

        duels_category_meta(value).map(|meta| Self::Category(meta.division_id))
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuelsWinstreak {
    Unknown,
    Value(u64),
}


impl Default for DuelsWinstreak {
    fn default() -> Self {
        Self::Unknown
    }
}


impl DuelsWinstreak {
    pub fn value(self) -> Option<u64> {
        match self {
            Self::Unknown => None,
            Self::Value(value) => Some(value),
        }
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuelsCategoryMeta {
    pub division_id: &'static str,
    pub category_key: &'static str,
    pub display_name: &'static str,
    pub requirement: DivisionTrack,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuelsModeMeta {
    pub id: &'static str,
    pub division_id: &'static str,
    pub category_key: &'static str,
    pub category_name: &'static str,
    pub name: &'static str,
    pub requirement: DivisionTrack,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuelsDivision {
    pub req: u64,
    pub step: u64,
    pub max: u32,
    pub name: &'static str,
    pub color_name: &'static str,
    pub bold: bool,
}


const DIVISION_REQUIREMENTS: [DuelsDivision; 12] = [
    DuelsDivision {
        req: 0,
        step: 0,
        max: 5,
        name: "None",
        color_name: "GRAY",
        bold: false,
    },
    DuelsDivision {
        req: 50,
        step: 10,
        max: 5,
        name: "Rookie",
        color_name: "GRAY",
        bold: false,
    },
    DuelsDivision {
        req: 100,
        step: 30,
        max: 5,
        name: "Iron",
        color_name: "WHITE",
        bold: false,
    },
    DuelsDivision {
        req: 250,
        step: 50,
        max: 5,
        name: "Gold",
        color_name: "GOLD",
        bold: false,
    },
    DuelsDivision {
        req: 500,
        step: 100,
        max: 5,
        name: "Diamond",
        color_name: "DARK_AQUA",
        bold: false,
    },
    DuelsDivision {
        req: 1000,
        step: 200,
        max: 5,
        name: "Master",
        color_name: "DARK_GREEN",
        bold: false,
    },
    DuelsDivision {
        req: 2000,
        step: 600,
        max: 5,
        name: "Legend",
        color_name: "DARK_RED",
        bold: true,
    },
    DuelsDivision {
        req: 5000,
        step: 1000,
        max: 5,
        name: "Grandmaster",
        color_name: "YELLOW",
        bold: true,
    },
    DuelsDivision {
        req: 10000,
        step: 3000,
        max: 5,
        name: "Godlike",
        color_name: "DARK_PURPLE",
        bold: true,
    },
    DuelsDivision {
        req: 25000,
        step: 5000,
        max: 5,
        name: "CELESTIAL",
        color_name: "AQUA",
        bold: true,
    },
    DuelsDivision {
        req: 50000,
        step: 10000,
        max: 5,
        name: "DIVINE",
        color_name: "LIGHT_PURPLE",
        bold: true,
    },
    DuelsDivision {
        req: 100000,
        step: 10000,
        max: 50,
        name: "ASCENDED",
        color_name: "RED",
        bold: true,
    },
];


pub const DUELS_CATEGORIES: [DuelsCategoryMeta; 16] = [
    DuelsCategoryMeta {
        division_id: "uhc",
        category_key: "uhc",
        display_name: "UHC",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "op",
        category_key: "op",
        display_name: "OP",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "skywars",
        category_key: "skywars",
        display_name: "SkyWars",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "bow",
        category_key: "bow",
        display_name: "Bow",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "blitz",
        category_key: "blitz",
        display_name: "Blitz",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "sumo",
        category_key: "sumo",
        display_name: "Sumo",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "combo",
        category_key: "combo",
        display_name: "Combo",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "quakecraft",
        category_key: "quake",
        display_name: "Quakecraft",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "classic",
        category_key: "classic",
        display_name: "Classic",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "mega_walls",
        category_key: "mega_walls",
        display_name: "Mega Walls",
        requirement: DivisionTrack::Half,
    },
    DuelsCategoryMeta {
        division_id: "parkour",
        category_key: "parkour",
        display_name: "Parkour",
        requirement: DivisionTrack::Half,
    },
    DuelsCategoryMeta {
        division_id: "boxing",
        category_key: "boxing",
        display_name: "Boxing",
        requirement: DivisionTrack::Half,
    },
    DuelsCategoryMeta {
        division_id: "no_debuff",
        category_key: "no_debuff",
        display_name: "NoDebuff",
        requirement: DivisionTrack::Half,
    },
    DuelsCategoryMeta {
        division_id: "bridge",
        category_key: "bridge",
        display_name: "The Bridge",
        requirement: DivisionTrack::Half,
    },
    DuelsCategoryMeta {
        division_id: "spleef",
        category_key: "spleef",
        display_name: "Spleef",
        requirement: DivisionTrack::Default,
    },
    DuelsCategoryMeta {
        division_id: "bedwars",
        category_key: "bedwars",
        display_name: "Bed Wars",
        requirement: DivisionTrack::Default,
    },
];


pub const DUELS_MODES: [DuelsModeMeta; 32] = [
    DuelsModeMeta {
        id: "uhc_duel",
        division_id: "uhc",
        category_key: "uhc",
        category_name: "UHC",
        name: "UHC 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "uhc_doubles",
        division_id: "uhc",
        category_key: "uhc",
        category_name: "UHC",
        name: "UHC 2v2",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "uhc_four",
        division_id: "uhc",
        category_key: "uhc",
        category_name: "UHC",
        name: "UHC 4v4",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "uhc_meetup",
        division_id: "uhc",
        category_key: "uhc",
        category_name: "UHC",
        name: "UHC Deathmatch",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "op_duel",
        division_id: "op",
        category_key: "op",
        category_name: "OP",
        name: "OP 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "op_doubles",
        division_id: "op",
        category_key: "op",
        category_name: "OP",
        name: "OP 2v2",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "sw_duel",
        division_id: "skywars",
        category_key: "skywars",
        category_name: "SkyWars",
        name: "SkyWars 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "sw_doubles",
        division_id: "skywars",
        category_key: "skywars",
        category_name: "SkyWars",
        name: "SkyWars 2v2",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "bow_duel",
        division_id: "bow",
        category_key: "bow",
        category_name: "Bow",
        name: "Bow 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "blitz_duel",
        division_id: "blitz",
        category_key: "blitz",
        category_name: "Blitz",
        name: "Blitz 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "sumo_duel",
        division_id: "sumo",
        category_key: "sumo",
        category_name: "Sumo",
        name: "Sumo 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "combo_duel",
        division_id: "combo",
        category_key: "combo",
        category_name: "Combo",
        name: "Combo 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "quake_duel",
        division_id: "quakecraft",
        category_key: "quake",
        category_name: "Quakecraft",
        name: "Quakecraft 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "classic_duel",
        division_id: "classic",
        category_key: "classic",
        category_name: "Classic",
        name: "Classic 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "classic_doubles",
        division_id: "classic",
        category_key: "classic",
        category_name: "Classic",
        name: "Classic 2v2",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "mw_duel",
        division_id: "mega_walls",
        category_key: "mega_walls",
        category_name: "Mega Walls",
        name: "MegaWalls 1v1",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "mw_doubles",
        division_id: "mega_walls",
        category_key: "mega_walls",
        category_name: "Mega Walls",
        name: "MegaWalls 2v2",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "parkour_eight",
        division_id: "parkour",
        category_key: "parkour",
        category_name: "Parkour",
        name: "Parkour FFA",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "boxing_duel",
        division_id: "boxing",
        category_key: "boxing",
        category_name: "Boxing",
        name: "Boxing 1v1",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "potion_duel",
        division_id: "no_debuff",
        category_key: "no_debuff",
        category_name: "NoDebuff",
        name: "NoDebuff 1v1",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "bridge_duel",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge 1v1",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "bridge_doubles",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge 2v2",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "bridge_threes",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge 3v3",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "bridge_four",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge 4v4",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "bridge_2v2v2v2",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge 2v2v2v2",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "bridge_3v3v3v3",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge 3v3v3v3",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "capture_threes",
        division_id: "bridge",
        category_key: "bridge",
        category_name: "The Bridge",
        name: "Bridge CTF 3v3",
        requirement: DivisionTrack::Half,
    },
    DuelsModeMeta {
        id: "spleef_duel",
        division_id: "spleef",
        category_key: "spleef",
        category_name: "Spleef",
        name: "Spleef 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "bowspleef_duel",
        division_id: "spleef",
        category_key: "spleef",
        category_name: "Spleef",
        name: "Bow Spleef 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "bedwars_two_one_duels",
        division_id: "bedwars",
        category_key: "bedwars",
        category_name: "Bed Wars",
        name: "Bed Wars 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "bedwars_two_one_duels_rush",
        division_id: "bedwars",
        category_key: "bedwars",
        category_name: "Bed Wars",
        name: "Bed Rush 1v1",
        requirement: DivisionTrack::Default,
    },
    DuelsModeMeta {
        id: "duel_arena",
        division_id: "",
        category_key: "arena",
        category_name: "Duel Arena",
        name: "Duel Arena",
        requirement: DivisionTrack::Default,
    },
];


#[derive(Clone, Default)]
pub struct DuelsModeStats {
    pub wins: u64,
    pub losses: u64,
    pub kills: u64,
    pub deaths: u64,
    pub bridge_kills: u64,
    pub bridge_deaths: u64,
    pub melee_hits: u64,
    pub melee_swings: u64,
    pub bow_hits: u64,
    pub bow_shots: u64,
    pub goals: u64,
    pub current_winstreak: DuelsWinstreak,
    pub best_winstreak: DuelsWinstreak,
}


impl DuelsModeStats {
    pub fn effective_kills(&self) -> u64 {
        if self.bridge_kills > 0 {
            self.bridge_kills
        } else {
            self.kills
        }
    }

    pub fn effective_deaths(&self) -> u64 {
        if self.bridge_deaths > 0 {
            self.bridge_deaths
        } else {
            self.deaths
        }
    }

    pub fn has_activity(&self) -> bool {
        self.wins > 0
            || self.losses > 0
            || self.kills > 0
            || self.deaths > 0
            || self.bridge_kills > 0
            || self.bridge_deaths > 0
    }
}


#[derive(Clone, Default)]
pub struct DuelsOverview {
    pub wins: u64,
    pub losses: u64,
    pub kills: u64,
    pub deaths: u64,
    pub melee_hits: u64,
    pub melee_swings: u64,
    pub bow_hits: u64,
    pub bow_shots: u64,
    pub current_winstreak: DuelsWinstreak,
    pub best_winstreak: DuelsWinstreak,
}


#[derive(Clone)]
pub struct DuelsCategoryStats {
    pub meta: DuelsCategoryMeta,
    pub wins: u64,
    pub losses: u64,
    pub kills: u64,
    pub deaths: u64,
    pub melee_hits: u64,
    pub melee_swings: u64,
    pub bow_hits: u64,
    pub bow_shots: u64,
    pub goals: u64,
    pub current_winstreak: DuelsWinstreak,
    pub best_winstreak: DuelsWinstreak,
    pub modes: Vec<(DuelsModeMeta, DuelsModeStats)>,
}


impl DuelsCategoryStats {
    pub fn has_activity(&self) -> bool {
        self.wins > 0 || self.losses > 0 || self.kills > 0 || self.deaths > 0
    }
}


#[derive(Clone)]
pub struct DuelsBreakdownEntry {
    pub label: String,
    pub division: (DuelsDivision, u32),
    pub wins: u64,
    pub losses: u64,
    pub kills: u64,
    pub deaths: u64,
    pub melee_hits: u64,
    pub melee_swings: u64,
    pub bow_hits: u64,
    pub bow_shots: u64,
    pub goals: u64,
    pub current_winstreak: DuelsWinstreak,
    pub best_winstreak: DuelsWinstreak,
}


#[derive(Clone)]
pub struct DuelsViewStats {
    pub view: DuelsView,
    pub title: String,
    pub division: (DuelsDivision, u32),
    pub wins: u64,
    pub losses: u64,
    pub kills: u64,
    pub deaths: u64,
    pub melee_hits: u64,
    pub melee_swings: u64,
    pub bow_hits: u64,
    pub bow_shots: u64,
    pub goals: u64,
    pub current_winstreak: DuelsWinstreak,
    pub best_winstreak: DuelsWinstreak,
    pub show_goals: bool,
    pub breakdown_title: &'static str,
    pub breakdown: Vec<DuelsBreakdownEntry>,
    pub track: DivisionTrack,
}


#[derive(Clone)]
pub struct DuelsStats {
    pub username: String,
    pub display_name: String,
    pub rank_prefix: Option<String>,
    pub network_level: f64,
    pub first_login: Option<i64>,
    pub guild: GuildInfo,
    pub overview: DuelsOverview,
    pub categories: Vec<DuelsCategoryStats>,
    pub modes: Vec<(DuelsModeMeta, DuelsModeStats)>,
    pub coins: u64,
    pub ping_preference: Option<u64>,
    pub damage_dealt: u64,
    pub blocks_placed: u64,
}


impl DuelsStats {
    pub fn active_views(&self) -> Vec<DuelsView> {
        let mut views = vec![DuelsView::Overall];
        views.extend(
            self.categories
                .iter()
                .filter(|category| category.has_activity())
                .map(|category| DuelsView::Category(category.meta.division_id)),
        );
        views
    }

    pub fn view_stats(&self, view: DuelsView) -> Option<DuelsViewStats> {
        match view {
            DuelsView::Overall => Some(self.overall_view()),
            DuelsView::Category(division_id) => self.category_view(division_id),
        }
    }

    pub fn default_view(&self) -> DuelsView {
        DuelsView::Overall
    }

    fn overall_view(&self) -> DuelsViewStats {
        let mut breakdown = self
            .categories
            .iter()
            .filter(|category| category.has_activity())
            .map(|category| DuelsBreakdownEntry {
                label: category.meta.display_name.to_string(),
                division: division_for_wins(category.wins, category.meta.requirement),
                wins: category.wins,
                losses: category.losses,
                kills: category.kills,
                deaths: category.deaths,
                melee_hits: category.melee_hits,
                melee_swings: category.melee_swings,
                bow_hits: category.bow_hits,
                bow_shots: category.bow_shots,
                goals: category.goals,
                current_winstreak: category.current_winstreak,
                best_winstreak: category.best_winstreak,
            })
            .collect::<Vec<_>>();

        breakdown.sort_by(|a, b| (b.wins + b.losses).cmp(&(a.wins + a.losses)));

        DuelsViewStats {
            view: DuelsView::Overall,
            title: "Overall".to_string(),
            division: division_for_wins(self.overview.wins, DivisionTrack::Overall),
            wins: self.overview.wins,
            losses: self.overview.losses,
            kills: self.overview.kills,
            deaths: self.overview.deaths,
            melee_hits: self.overview.melee_hits,
            melee_swings: self.overview.melee_swings,
            bow_hits: self.overview.bow_hits,
            bow_shots: self.overview.bow_shots,
            goals: self.modes.iter().map(|(_, mode)| mode.goals).sum(),
            current_winstreak: self.overview.current_winstreak,
            best_winstreak: self.overview.best_winstreak,
            show_goals: true,
            breakdown_title: "Top Played",
            breakdown,
            track: DivisionTrack::Overall,
        }
    }

    fn category_view(&self, division_id: &str) -> Option<DuelsViewStats> {
        let category = self
            .categories
            .iter()
            .find(|category| category.meta.division_id == division_id)?;

        let division = division_for_wins(category.wins, category.meta.requirement);
        let mut breakdown = category
            .modes
            .iter()
            .filter(|(_, mode)| mode.has_activity())
            .map(|(meta, mode)| DuelsBreakdownEntry {
                label: meta.name.to_string(),
                division,
                wins: mode.wins,
                losses: mode.losses,
                kills: mode.effective_kills(),
                deaths: mode.effective_deaths(),
                melee_hits: mode.melee_hits,
                melee_swings: mode.melee_swings,
                bow_hits: mode.bow_hits,
                bow_shots: mode.bow_shots,
                goals: mode.goals,
                current_winstreak: mode.current_winstreak,
                best_winstreak: mode.best_winstreak,
            })
            .collect::<Vec<_>>();

        breakdown.sort_by(|a, b| (b.wins + b.losses).cmp(&(a.wins + a.losses)));

        Some(DuelsViewStats {
            view: DuelsView::Category(category.meta.division_id),
            title: category.meta.display_name.to_string(),
            division,
            wins: category.wins,
            losses: category.losses,
            kills: category.kills,
            deaths: category.deaths,
            melee_hits: category.melee_hits,
            melee_swings: category.melee_swings,
            bow_hits: category.bow_hits,
            bow_shots: category.bow_shots,
            goals: category.goals,
            current_winstreak: category.current_winstreak,
            best_winstreak: category.best_winstreak,
            show_goals: category.meta.division_id == "bridge",
            breakdown_title: "Top Played",
            breakdown,
            track: category.meta.requirement,
        })
    }
}


pub fn duels_category_meta(division_id: &str) -> Option<DuelsCategoryMeta> {
    DUELS_CATEGORIES
        .iter()
        .copied()
        .find(|meta| meta.division_id == division_id)
}


pub fn division_for_wins(wins: u64, track: DivisionTrack) -> (DuelsDivision, u32) {
    let adjusted = match track {
        DivisionTrack::Default => wins,
        DivisionTrack::Half => wins * 2,
        DivisionTrack::Overall => wins / 2,
    };

    let mut division = DIVISION_REQUIREMENTS[0];
    for candidate in DIVISION_REQUIREMENTS {
        if adjusted >= candidate.req {
            division = candidate;
        } else {
            break;
        }
    }

    if division.name == "None" {
        return (division, 0);
    }

    let remaining = adjusted.saturating_sub(division.req);
    let raw_level = if division.step == 0 {
        1
    } else {
        remaining / division.step + 1
    };

    (division, raw_level.min(division.max as u64) as u32)
}


pub fn division_progress(wins: u64, track: DivisionTrack) -> f64 {
    let adjusted = match track {
        DivisionTrack::Default => wins,
        DivisionTrack::Half => wins * 2,
        DivisionTrack::Overall => wins / 2,
    };

    let (division, level) = division_for_wins(wins, track);

    if division.name == "None" || level == 0 {
        return (adjusted as f64 / 50.0).clamp(0.0, 1.0);
    }

    let div_index = DIVISION_REQUIREMENTS
        .iter()
        .position(|d| d.name == division.name)
        .unwrap_or(0);

    if div_index == DIVISION_REQUIREMENTS.len() - 1 && level >= division.max {
        return 1.0;
    }

    let current_threshold = if level == 1 {
        division.req
    } else {
        division.req + (level as u64 - 1) * division.step
    };

    let next_threshold = if level < division.max {
        division.req + level as u64 * division.step
    } else {
        DIVISION_REQUIREMENTS[div_index + 1].req
    };

    if next_threshold <= current_threshold {
        return 1.0;
    }

    ((adjusted as f64 - current_threshold as f64) / (next_threshold as f64 - current_threshold as f64))
        .clamp(0.0, 1.0)
}


pub fn next_division(division: DuelsDivision, level: u32) -> Option<(DuelsDivision, u32)> {
    if division.name == "None" || level == 0 {
        return Some((DIVISION_REQUIREMENTS[1], 1));
    }

    if level < division.max {
        return Some((division, level + 1));
    }

    let div_index = DIVISION_REQUIREMENTS
        .iter()
        .position(|d| d.name == division.name)?;

    if div_index + 1 < DIVISION_REQUIREMENTS.len() {
        Some((DIVISION_REQUIREMENTS[div_index + 1], 1))
    } else {
        None // At ASCENDED max
    }
}


pub fn win_progress(wins: u64, track: DivisionTrack) -> (u64, u64) {
    let adjusted = match track {
        DivisionTrack::Default => wins,
        DivisionTrack::Half => wins * 2,
        DivisionTrack::Overall => wins / 2,
    };
    let (division, level) = division_for_wins(wins, track);

    if division.name == "None" || level == 0 {
        return (adjusted, 50);
    }

    let div_index = DIVISION_REQUIREMENTS.iter().position(|d| d.name == division.name).unwrap_or(0);

    if div_index == DIVISION_REQUIREMENTS.len() - 1 && level >= division.max {
        return (adjusted, adjusted);
    }

    let next_threshold = if level < division.max {
        division.req + level as u64 * division.step
    } else {
        DIVISION_REQUIREMENTS[div_index + 1].req
    };

    (adjusted, next_threshold)
}


pub fn extract_duels_stats(
    username: &str,
    player: &Value,
    guild: Option<GuildInfo>,
) -> Option<DuelsStats> {
    let duels = player.get("stats")?.get("Duels")?;
    let display_name = player
        .get("displayname")
        .and_then(|value| value.as_str())
        .unwrap_or(username)
        .to_string();
    let network_exp = player
        .get("networkExp")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);

    let modes: Vec<_> = DUELS_MODES
        .iter()
        .copied()
        .map(|meta| {
            let stats = DuelsModeStats {
                wins: stat(duels, &format!("{}_wins", meta.id)),
                losses: stat(duels, &format!("{}_losses", meta.id)),
                kills: stat(duels, &format!("{}_kills", meta.id)),
                deaths: stat(duels, &format!("{}_deaths", meta.id)),
                bridge_kills: stat(duels, &format!("{}_bridge_kills", meta.id)),
                bridge_deaths: stat(duels, &format!("{}_bridge_deaths", meta.id)),
                melee_hits: stat(duels, &format!("{}_melee_hits", meta.id)),
                melee_swings: stat(duels, &format!("{}_melee_swings", meta.id)),
                bow_hits: stat(duels, &format!("{}_bow_hits", meta.id)),
                bow_shots: stat(duels, &format!("{}_bow_shots", meta.id)),
                goals: stat(duels, &format!("{}_goals", meta.id)),
                current_winstreak: winstreak_value(
                    duels,
                    &format!("current_winstreak_mode_{}", meta.id),
                ),
                best_winstreak: winstreak_value(duels, &format!("best_winstreak_mode_{}", meta.id)),
            };
            (meta, stats)
        })
        .collect();

    let categories = DUELS_CATEGORIES
        .iter()
        .copied()
        .map(|meta| {
            let category_modes: Vec<_> = modes
                .iter()
                .filter(|(mode_meta, _)| mode_meta.division_id == meta.division_id)
                .map(|(mode_meta, mode_stats)| (*mode_meta, mode_stats.clone()))
                .collect();

            let wins = category_modes.iter().map(|(_, mode)| mode.wins).sum();
            let losses = category_modes.iter().map(|(_, mode)| mode.losses).sum();
            let kills = category_modes
                .iter()
                .map(|(_, mode)| mode.effective_kills())
                .sum();
            let deaths = category_modes
                .iter()
                .map(|(_, mode)| mode.effective_deaths())
                .sum();
            let melee_hits = category_modes.iter().map(|(_, mode)| mode.melee_hits).sum();
            let melee_swings = category_modes.iter().map(|(_, mode)| mode.melee_swings).sum();
            let bow_hits = category_modes.iter().map(|(_, mode)| mode.bow_hits).sum();
            let bow_shots = category_modes.iter().map(|(_, mode)| mode.bow_shots).sum();
            let goals = category_modes.iter().map(|(_, mode)| mode.goals).sum();

            DuelsCategoryStats {
                meta,
                wins,
                losses,
                kills,
                deaths,
                melee_hits,
                melee_swings,
                bow_hits,
                bow_shots,
                goals,
                current_winstreak: winstreak_value(
                    duels,
                    &format!("current_{}_winstreak", meta.category_key),
                ),
                best_winstreak: winstreak_value(
                    duels,
                    &format!("best_{}_winstreak", meta.category_key),
                ),
                modes: category_modes,
            }
        })
        .collect();

    let overview = DuelsOverview {
        wins: stat(duels, "wins"),
        losses: stat(duels, "losses"),
        kills: stat(duels, "kills"),
        deaths: stat(duels, "deaths"),
        melee_hits: DUELS_MODES
            .iter()
            .map(|mode| stat(duels, &format!("{}_melee_hits", mode.id)))
            .sum(),
        melee_swings: DUELS_MODES
            .iter()
            .map(|mode| stat(duels, &format!("{}_melee_swings", mode.id)))
            .sum(),
        bow_hits: DUELS_MODES
            .iter()
            .map(|mode| stat(duels, &format!("{}_bow_hits", mode.id)))
            .sum(),
        bow_shots: DUELS_MODES
            .iter()
            .map(|mode| stat(duels, &format!("{}_bow_shots", mode.id)))
            .sum(),
        current_winstreak: overall_winstreak(duels, "current"),
        best_winstreak: overall_winstreak(duels, "best"),
    };

    Some(DuelsStats {
        username: username.to_string(),
        display_name,
        rank_prefix: extract_rank_prefix(player),
        network_level: calculate_network_level(network_exp),
        first_login: player.get("firstLogin").and_then(|v| v.as_i64()),
        guild: guild.unwrap_or_default(),
        overview,
        categories,
        modes,
        coins: duels.get("coins").and_then(|v| v.as_u64()).unwrap_or(0),
        ping_preference: duels.get("pingPreference").and_then(|v| v.as_u64()),
        damage_dealt: stat(duels, "damage_dealt"),
        blocks_placed: stat(duels, "blocks_placed"),
    })
}


fn stat(json: &Value, key: &str) -> u64 {
    json.get(key).and_then(|value| value.as_u64()).unwrap_or(0)
}


fn winstreak_value(json: &Value, key: &str) -> DuelsWinstreak {
    match json.get(key).and_then(|value| value.as_u64()) {
        Some(value) => DuelsWinstreak::Value(value),
        None => DuelsWinstreak::Unknown,
    }
}


fn overall_winstreak(json: &Value, kind: &str) -> DuelsWinstreak {
    let keys: &[&str] = match kind {
        "current" => &[
            "currentStreak",
            "current_winstreak",
            "current_all_modes_winstreak",
        ],
        "best" => &["best_overall_winstreak", "best_all_modes_winstreak"],
        _ => &[],
    };

    keys.iter()
        .find_map(|key| json.get(key).and_then(|value| value.as_u64()))
        .map(DuelsWinstreak::Value)
        .unwrap_or(DuelsWinstreak::Unknown)
}


#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DivisionTrack, DuelsView, DuelsWinstreak, division_for_wins, extract_duels_stats};
    use crate::GuildInfo;

    #[test]
    fn division_thresholds_match_expected_tracks() {
        let (_, default_level) = division_for_wins(50, DivisionTrack::Default);
        let (_, half_level) = division_for_wins(50, DivisionTrack::Half);
        let (overall_division, overall_level) = division_for_wins(100, DivisionTrack::Overall);

        assert_eq!(default_level, 1);
        assert_eq!(half_level, 1);
        assert_eq!(overall_division.name, "Rookie");
        assert_eq!(overall_level, 1);
    }

    #[test]
    fn extracts_bridge_and_category_stats() {
        let player = json!({
            "displayname": "Tester",
            "networkExp": 10000.0,
            "stats": {
                "Duels": {
                    "wins": 300,
                    "losses": 120,
                    "kills": 500,
                    "deaths": 250,
                    "current_all_modes_winstreak": 8,
                    "best_all_modes_winstreak": 20,
                    "current_bridge_winstreak": 4,
                    "best_bridge_winstreak": 12,
                    "uhc_duel_wins": 100,
                    "uhc_duel_losses": 25,
                    "bridge_duel_wins": 70,
                    "bridge_duel_losses": 10,
                    "bridge_duel_goals": 11,
                    "bridge_duel_bridge_kills": 40,
                    "bridge_duel_bridge_deaths": 8,
                    "bridge_duel_kills": 1,
                    "bridge_duel_deaths": 1
                }
            }
        });

        let stats = extract_duels_stats("Tester", &player, Some(GuildInfo::default())).unwrap();
        let bridge = stats
            .view_stats(DuelsView::Category("bridge"))
            .expect("bridge category");

        assert_eq!(bridge.wins, 70);
        assert_eq!(bridge.kills, 40);
        assert_eq!(bridge.deaths, 8);
        assert_eq!(bridge.goals, 11);
        assert_eq!(bridge.current_winstreak, DuelsWinstreak::Value(4));
        assert_eq!(bridge.best_winstreak, DuelsWinstreak::Value(12));
        assert_eq!(stats.overview.current_winstreak, DuelsWinstreak::Value(8));
        assert_eq!(stats.overview.best_winstreak, DuelsWinstreak::Value(20));
    }

    #[test]
    fn active_views_exclude_arena_only_category() {
        let player = json!({
            "displayname": "Tester",
            "networkExp": 10000.0,
            "stats": {
                "Duels": {
                    "wins": 5,
                    "losses": 1,
                    "kills": 3,
                    "deaths": 2,
                    "duel_arena_wins": 5,
                    "duel_arena_losses": 1,
                    "duel_arena_kills": 3,
                    "duel_arena_deaths": 2
                }
            }
        });

        let stats = extract_duels_stats("Tester", &player, Some(GuildInfo::default())).unwrap();
        let views = stats.active_views();

        assert_eq!(views, vec![DuelsView::Overall]);
    }
}
