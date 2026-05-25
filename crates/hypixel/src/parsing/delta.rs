use chrono::{DateTime, Utc};

use super::bedwars::{Mode, ModeStats, Stats, ratio};

#[derive(Clone, Default)]
pub struct SessionStats {
    pub wins: u64,
    pub losses: u64,
    pub kills: u64,
    pub deaths: u64,
    pub final_kills: u64,
    pub final_deaths: u64,
    pub beds_broken: u64,
    pub beds_lost: u64,
    pub experience: u64,
    pub games_played: u64,
}

impl SessionStats {
    pub fn from_mode_stats(old: &ModeStats, new: &ModeStats) -> Self {
        Self {
            wins: new.wins.saturating_sub(old.wins),
            losses: new.losses.saturating_sub(old.losses),
            kills: new.kills.saturating_sub(old.kills),
            deaths: new.deaths.saturating_sub(old.deaths),
            final_kills: new.final_kills.saturating_sub(old.final_kills),
            final_deaths: new.final_deaths.saturating_sub(old.final_deaths),
            beds_broken: new.beds_broken.saturating_sub(old.beds_broken),
            beds_lost: new.beds_lost.saturating_sub(old.beds_lost),
            experience: 0,
            games_played: 0,
        }
    }

    pub fn wlr(&self) -> f64 {
        ratio(self.wins, self.losses)
    }
    pub fn kdr(&self) -> f64 {
        ratio(self.kills, self.deaths)
    }
    pub fn fkdr(&self) -> f64 {
        ratio(self.final_kills, self.final_deaths)
    }
    pub fn bblr(&self) -> f64 {
        ratio(self.beds_broken, self.beds_lost)
    }
}

pub struct SessionPlayerStats {
    pub username: String,
    pub display_name: String,
    pub rank_prefix: Option<String>,
    pub experience: u64,
    pub level_progress: f64,
    pub games_played: u64,
    pub started: DateTime<Utc>,
    pub session_stats: SessionStats,
}

impl SessionPlayerStats {
    pub fn from_snapshots(old: &Stats, new: &Stats, mode: Mode, started: DateTime<Utc>) -> Self {
        let mut session_stats =
            SessionStats::from_mode_stats(&old.get_mode_stats(mode), &new.get_mode_stats(mode));
        session_stats.experience = new.experience.saturating_sub(old.experience);
        session_stats.games_played = new.games_played.saturating_sub(old.games_played);

        Self {
            username: new.username.clone(),
            display_name: new.display_name.clone(),
            rank_prefix: new.rank_prefix.clone(),
            experience: session_stats.experience,
            level_progress: session_stats.experience as f64 / 5000.0,
            games_played: session_stats.games_played,
            started,
            session_stats,
        }
    }
}
