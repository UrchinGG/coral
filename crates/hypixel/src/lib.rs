mod guild;
pub mod parsing;
mod player;
mod stats;

pub use guild::*;
pub use player::*;
pub use stats::*;

pub use parsing::bedwars::{
    GuildInfo, Mode, ModeStats, SlumberInfo, Stats as BedwarsPlayerStats,
    WinstreakModeData, WinstreakSnapshot, combined_mode_name,
    calculate_level, experience_for_level, extract as extract_bedwars_stats,
    extract_winstreak_snapshot, level_progress, ratio,
};
pub use parsing::delta::{SessionPlayerStats, SessionStats};
pub use parsing::duels::{
    DUELS_CATEGORIES, DUELS_MODES, DivisionTrack, DuelsBreakdownEntry, DuelsCategoryMeta,
    DuelsCategoryStats, DuelsDivision, DuelsModeMeta, DuelsModeStats, DuelsOverview, DuelsStats,
    DuelsView, DuelsViewStats, DuelsWinstreak, division_for_wins, division_progress,
    duels_category_meta, extract_duels_stats, next_division,
};
pub use parsing::duels_winstreaks::{
    DuelsWinstreakSnapshot, extract_duels_winstreak_snapshot,
};
pub use parsing::player::{calculate_network_level, color_code, extract_rank_prefix};
pub use parsing::winstreaks::{Streak, StreakSource, WinstreakHistory};
