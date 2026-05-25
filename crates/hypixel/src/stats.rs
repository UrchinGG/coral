use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStats {
    pub bedwars: Option<BedwarsStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedwarsStats {
    pub level: u32,
    pub coins: u64,
    pub games_played: u32,
    pub wins: u32,
    pub losses: u32,
    pub winstreak: Option<u32>,
    pub kills: u32,
    pub deaths: u32,
    pub final_kills: u32,
    pub final_deaths: u32,
    pub beds_broken: u32,
    pub beds_lost: u32,
}

impl BedwarsStats {
    pub fn win_rate(&self) -> f64 {
        if self.games_played == 0 {
            return 0.0;
        }
        self.wins as f64 / self.games_played as f64
    }

    pub fn kdr(&self) -> f64 {
        if self.deaths == 0 {
            return self.kills as f64;
        }
        self.kills as f64 / self.deaths as f64
    }

    pub fn fkdr(&self) -> f64 {
        if self.final_deaths == 0 {
            return self.final_kills as f64;
        }
        self.final_kills as f64 / self.final_deaths as f64
    }

    pub fn bblr(&self) -> f64 {
        if self.beds_lost == 0 {
            return self.beds_broken as f64;
        }
        self.beds_broken as f64 / self.beds_lost as f64
    }
}
