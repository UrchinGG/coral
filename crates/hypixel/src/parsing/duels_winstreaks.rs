use chrono::{DateTime, Utc};
use serde_json::Value;

use super::duels::{DUELS_CATEGORIES, DUELS_MODES, DuelsView};
use super::winstreaks::{Streak, StreakSource, WinstreakHistory};

const MIN_STREAK_THRESHOLD: u64 = 15;

#[derive(Clone)]
pub struct DuelsCategorySnapshot {
    pub division_id: &'static str,
    pub wins: u64,
    pub losses: u64,
    pub winstreak: Option<u64>,
    pub best_winstreak: Option<u64>,
}

#[derive(Clone, Default)]
pub struct DuelsWinstreakSnapshot {
    pub overall_wins: u64,
    pub overall_losses: u64,
    pub overall_winstreak: Option<u64>,
    pub overall_best_winstreak: Option<u64>,
    pub categories: Vec<DuelsCategorySnapshot>,
}

pub fn extract_duels_winstreak_snapshot(player: &Value) -> Option<DuelsWinstreakSnapshot> {
    let duels = player.get("stats")?.get("Duels")?;

    let overall_wins = stat(duels, "wins");
    let overall_losses = stat(duels, "losses");

    let overall_winstreak = [
        "currentStreak",
        "current_winstreak",
        "current_all_modes_winstreak",
    ]
    .iter()
    .find_map(|key| duels.get(key).and_then(|v| v.as_u64()));

    let overall_best_winstreak = duels.get("best_overall_winstreak").and_then(|v| v.as_u64());

    let categories = DUELS_CATEGORIES
        .iter()
        .map(|meta| {
            let wins: u64 = DUELS_MODES
                .iter()
                .filter(|mode| mode.division_id == meta.division_id)
                .map(|mode| stat(duels, &format!("{}_wins", mode.id)))
                .sum();

            let losses: u64 = DUELS_MODES
                .iter()
                .filter(|mode| mode.division_id == meta.division_id)
                .map(|mode| stat(duels, &format!("{}_losses", mode.id)))
                .sum();

            let winstreak = duels
                .get(&format!("current_{}_winstreak", meta.category_key))
                .and_then(|v| v.as_u64());

            let best_winstreak = duels
                .get(&format!("best_{}_winstreak", meta.category_key))
                .and_then(|v| v.as_u64());

            DuelsCategorySnapshot {
                division_id: meta.division_id,
                wins,
                losses,
                winstreak,
                best_winstreak,
            }
        })
        .collect();

    Some(DuelsWinstreakSnapshot {
        overall_wins,
        overall_losses,
        overall_winstreak,
        overall_best_winstreak,
        categories,
    })
}

pub fn calculate(
    snapshots: &[(DateTime<Utc>, DuelsWinstreakSnapshot)],
    view: DuelsView,
) -> WinstreakHistory {
    if snapshots.is_empty() {
        return WinstreakHistory {
            streaks: Vec::new(),
        };
    }

    let mut streaks = Vec::new();
    let mut streak_start: Option<usize> = None;
    let mut peak_api_ws: Option<u64> = None;

    for (i, (_ts, stats)) in snapshots.iter().enumerate() {
        let (wins, losses) = view_wins_losses(stats, view);
        let api_ws = api_winstreak(stats, view);

        let delta_losses = if i > 0 {
            let (_, prev_losses) = view_wins_losses(&snapshots[i - 1].1, view);
            losses.saturating_sub(prev_losses)
        } else {
            0
        };

        if let Some(ws) = api_ws {
            peak_api_ws = Some(peak_api_ws.map_or(ws, |peak| peak.max(ws)));
        }

        if let (Some(start_idx), true) = (streak_start, delta_losses > 0) {
            let prev_idx = i - 1;
            let prev_ts = snapshots[prev_idx].0;
            let (start_wins, _) = view_wins_losses(&snapshots[start_idx].1, view);
            let (prev_wins, _) = view_wins_losses(&snapshots[prev_idx].1, view);

            let (value, approximate) = match peak_api_ws {
                Some(peak) => {
                    let delta_wins = wins.saturating_sub(prev_wins);
                    let mut best = peak;
                    if delta_losses == 1 {
                        best = match api_ws {
                            Some(after) => {
                                best.max(wins.saturating_sub(start_wins).saturating_sub(after))
                            }
                            None => best.max(prev_wins.saturating_sub(start_wins)),
                        };
                    } else {
                        best = best.max(prev_wins.saturating_sub(start_wins));
                    }
                    (best, delta_wins >= 2 || best > peak)
                }
                None => (prev_wins.saturating_sub(start_wins), true),
            };

            // A new personal best recorded inside this gap is the streak that just ended
            // whenever no streak that began after a loss could have been long enough to
            // set it, which makes the value exact rather than inferred.
            let (value, approximate) = match (
                best_winstreak(&snapshots[prev_idx].1, view),
                best_winstreak(stats, view),
            ) {
                (Some(before), Some(after))
                    if after > before
                        && after > wins.saturating_sub(prev_wins)
                        && after >= value =>
                {
                    (after, false)
                }
                _ => (value, approximate),
            };

            if value >= MIN_STREAK_THRESHOLD {
                streaks.push(Streak {
                    value,
                    approximate,
                    timestamp: prev_ts,
                    source: StreakSource::Urchin,
                });
            }

            streak_start = None;
            peak_api_ws = None;
        }

        if streak_start.is_none() && (delta_losses > 0 || i == 0) {
            streak_start = Some(i);
            peak_api_ws = api_ws;
        }
    }

    streaks.sort_by(|a, b| b.value.cmp(&a.value));
    WinstreakHistory { streaks }
}

fn view_wins_losses(stats: &DuelsWinstreakSnapshot, view: DuelsView) -> (u64, u64) {
    match view {
        DuelsView::Overall => (stats.overall_wins, stats.overall_losses),
        DuelsView::Category(id) => stats
            .categories
            .iter()
            .find(|c| c.division_id == id)
            .map(|c| (c.wins, c.losses))
            .unwrap_or((0, 0)),
    }
}

fn api_winstreak(stats: &DuelsWinstreakSnapshot, view: DuelsView) -> Option<u64> {
    match view {
        DuelsView::Overall => stats.overall_winstreak,
        DuelsView::Category(id) => stats
            .categories
            .iter()
            .find(|c| c.division_id == id)
            .and_then(|c| c.winstreak),
    }
}

fn best_winstreak(stats: &DuelsWinstreakSnapshot, view: DuelsView) -> Option<u64> {
    match view {
        DuelsView::Overall => stats.overall_best_winstreak,
        DuelsView::Category(id) => stats
            .categories
            .iter()
            .find(|c| c.division_id == id)
            .and_then(|c| c.best_winstreak),
    }
}

fn stat(json: &Value, key: &str) -> u64 {
    json.get(key).and_then(|value| value.as_u64()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn snap(wins: u64, losses: u64, ws: Option<u64>, best: Option<u64>) -> DuelsWinstreakSnapshot {
        DuelsWinstreakSnapshot {
            overall_wins: wins,
            overall_losses: losses,
            overall_winstreak: ws,
            overall_best_winstreak: best,
            categories: Vec::new(),
        }
    }

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, minute, 0).unwrap()
    }

    #[test]
    fn new_best_in_a_gap_makes_the_streak_exact() {
        let snapshots = [
            (at(0), snap(100, 10, Some(0), Some(40))),
            (at(1), snap(197, 10, Some(97), Some(97))),
            (at(2), snap(201, 11, None, Some(100))),
        ];
        let history = calculate(&snapshots, DuelsView::Overall);
        assert_eq!(history.streaks.len(), 1);
        assert_eq!(history.streaks[0].value, 100);
        assert!(!history.streaks[0].approximate);
    }

    #[test]
    fn unchanged_best_stays_approximate() {
        let snapshots = [
            (at(0), snap(100, 10, Some(0), Some(100))),
            (at(1), snap(137, 10, Some(37), Some(100))),
            (at(2), snap(141, 11, None, Some(100))),
        ];
        let history = calculate(&snapshots, DuelsView::Overall);
        assert_eq!(history.streaks.len(), 1);
        assert_eq!(history.streaks[0].value, 37);
        assert!(history.streaks[0].approximate);
    }
}
