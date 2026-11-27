use std::{cmp::Reverse, collections::HashMap};

use replay_protocol::{TeamAction, TeamInfo, color_after};

use crate::strip;

const SIDEBAR: i8 = 1;

#[derive(Default)]
pub(crate) struct Scoreboard {
    teams: Vec<(String, Team)>,
    objectives: HashMap<String, HashMap<String, i32>>,
    sidebar: Option<String>,
}

struct Team {
    prefix: String,
    suffix: String,
    members: Vec<String>,
}

#[derive(Default)]
pub(crate) struct Sidebar {
    pub game: Option<String>,
    pub mode: Option<String>,
    pub map: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
}

impl Scoreboard {
    pub fn objective(&mut self, name: String, removed: bool) {
        if removed {
            self.objectives.remove(&name);
        } else {
            self.objectives.entry(name).or_default();
        }
    }

    pub fn score(&mut self, entry: String, objective: &str, value: Option<i32>) {
        if objective.is_empty() && value.is_none() {
            for scores in self.objectives.values_mut() {
                scores.remove(&entry);
            }
            return;
        }
        let Some(scores) = self.objectives.get_mut(objective) else {
            return;
        };
        match value {
            Some(value) => scores.insert(entry, value),
            None => scores.remove(&entry),
        };
    }

    pub fn display(&mut self, position: i8, objective: String) {
        if position == SIDEBAR {
            self.sidebar = Some(objective);
        }
    }

    pub fn team(&mut self, name: String, action: TeamAction) {
        match action {
            TeamAction::Remove => self.teams.retain(|(team, _)| *team != name),
            TeamAction::Create { info, players } => self.enroll(name, players).restyle(info),
            TeamAction::Update(info) => self.team_mut(name).restyle(info),
            TeamAction::AddPlayers(players) => {
                self.enroll(name, players);
            }
            TeamAction::RemovePlayers(players) => self
                .team_mut(name)
                .members
                .retain(|member| !players.contains(member)),
        }
    }

    fn enroll(&mut self, name: String, players: Vec<String>) -> &mut Team {
        for (_, team) in &mut self.teams {
            team.members.retain(|member| !players.contains(member));
        }
        let team = self.team_mut(name);
        team.add(players);
        team
    }

    fn team_mut(&mut self, name: String) -> &mut Team {
        let index = self
            .teams
            .iter()
            .position(|(team, _)| *team == name)
            .unwrap_or_else(|| {
                let team = Team {
                    prefix: "§f".into(),
                    suffix: String::new(),
                    members: Vec::new(),
                };
                self.teams.push((name, team));
                self.teams.len() - 1
            });
        &mut self.teams[index].1
    }

    pub fn team_color(&self, username: &str) -> Option<u32> {
        let (_, team) = self.teams.iter().find(|(_, team)| {
            team.members
                .iter()
                .any(|member| strip(member).eq_ignore_ascii_case(username))
        })?;
        color_after(&team.prefix)
    }

    fn sidebar_lines(&self) -> Vec<String> {
        let Some(scores) = self
            .sidebar
            .as_ref()
            .and_then(|name| self.objectives.get(name))
        else {
            return Vec::new();
        };
        let mut entries: Vec<_> = scores.iter().collect();
        entries.sort_unstable_by_key(|&(entry, &value)| (Reverse(value), entry));
        entries
            .into_iter()
            .map(|(entry, _)| {
                match self
                    .teams
                    .iter()
                    .find(|(_, team)| team.members.contains(entry))
                {
                    Some((_, team)) => format!("{}{entry}{}", team.prefix, team.suffix),
                    None => entry.clone(),
                }
            })
            .collect()
    }
}

impl Team {
    fn restyle(&mut self, info: TeamInfo) {
        self.prefix = info.prefix;
        self.suffix = info.suffix;
    }

    fn add(&mut self, players: Vec<String>) {
        for player in players {
            if !self.members.contains(&player) {
                self.members.push(player);
            }
        }
    }
}

impl Sidebar {
    pub fn read(&mut self, scoreboard: &Scoreboard) {
        for line in scoreboard.sidebar_lines() {
            let line: String = strip(&line)
                .chars()
                .filter(|&character| !is_emoji(character))
                .collect();
            let line = line.trim();
            let field = |label: &str| {
                let start = line.to_ascii_lowercase().find(label)? + label.len();
                Some(&line[start..])
            };
            let words = |label: &str| {
                let value = field(label)?
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                (!value.is_empty()).then_some(value)
            };
            self.date = field("date:")
                .and_then(|value| value.split_whitespace().next())
                .map(str::to_owned)
                .or(self.date.take());
            self.time = words("time:").or(self.time.take());
            self.game = words("game:").or(self.game.take());
            self.mode = words("mode:").or(self.mode.take());
            self.map = words("map:").or(self.map.take());
        }
    }
}

fn is_emoji(character: char) -> bool {
    matches!(character, '\u{200D}' | '\u{FE0F}' | '\u{2600}'..='\u{27BF}' | '\u{2B00}'..='\u{2BFF}' | '\u{1F000}'..='\u{1FAFF}')
}
