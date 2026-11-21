use replay_file::Recording;
use replay_protocol::{Clientbound, decode, flatten_text};

use crate::strip;

const FINISH_TAIL: u32 = 2500;
const END_GRACE: u32 = 2000;

pub(crate) struct Clock {
    pub start: u32,
    pub duration: u32,
    pub finish: u32,
    follows_action_bar: bool,
    paused_since: Option<u32>,
    paused_total: u32,
}

struct ActionBar {
    current: u32,
    duration: u32,
    state: Option<&'static str>,
}

impl Clock {
    pub fn new(recording: &Recording) -> Clock {
        let first_playing = recording
            .packets()
            .filter(|packet| packet.id == 0x02)
            .find_map(|packet| {
                let Some(Clientbound::Chat { json, position: 2 }) = decode(packet.id, packet.body)
                else {
                    return None;
                };
                let bar = ActionBar::parse(&json)?;
                (bar.state == Some("Playing")).then_some((packet.time, bar.duration))
            });
        let (start, duration, finish, follows_action_bar) = match first_playing {
            Some((start, duration)) => (
                start,
                duration,
                start.saturating_add(duration).saturating_add(FINISH_TAIL),
                true,
            ),
            None => (0, recording.metadata().duration_ms, u32::MAX, false),
        };
        Clock {
            start,
            duration,
            finish,
            follows_action_bar,
            paused_since: None,
            paused_total: 0,
        }
    }

    pub fn elapsed(&self, raw: u32) -> u32 {
        self.paused_since
            .unwrap_or(raw)
            .saturating_sub(self.start + self.paused_total)
    }

    pub fn read_action_bar(&mut self, raw: u32, json: &str) {
        let Some(bar) =
            ActionBar::parse(json).filter(|_| self.follows_action_bar && raw >= self.start)
        else {
            return;
        };
        let playing = bar.state == Some("Playing");
        match self.paused_since {
            None if !playing => self.paused_since = Some(raw),
            Some(since) if playing => {
                self.paused_total += raw - since;
                self.paused_since = None;
            }
            _ => {}
        }
        let stopped = matches!(bar.state, Some("Paused" | "Finished"));
        if stopped && bar.current >= self.duration.saturating_sub(END_GRACE) {
            self.finish = self.finish.min(raw);
        } else if bar.current >= self.duration {
            self.finish = self.finish.min(raw.saturating_add(END_GRACE));
        }
    }
}

impl ActionBar {
    fn parse(json: &str) -> Option<ActionBar> {
        let text = plain(json);
        let (current, duration) = text
            .char_indices()
            .find_map(|(index, _)| clock_at(&text[index..]))?;
        let state = ["Playing", "Paused", "Waiting", "Finished"]
            .into_iter()
            .find(|state| text.contains(state));
        (duration > 0).then_some(ActionBar {
            current,
            duration,
            state,
        })
    }
}

pub(crate) fn plain(json: &str) -> String {
    strip(&flatten_text(json))
}

pub(crate) fn clock_at(text: &str) -> Option<(u32, u32)> {
    let (current, rest) = minutes_seconds(text)?;
    let (duration, _) = minutes_seconds(rest.trim_start().strip_prefix('/')?.trim_start())?;
    Some((current, duration))
}

fn minutes_seconds(text: &str) -> Option<(u32, &str)> {
    let (minutes, rest) = number(text)?;
    let (seconds, rest) = number(rest.strip_prefix(':')?)?;
    let millis = (u64::from(minutes) * 60 + u64::from(seconds)) * 1000;
    Some((u32::try_from(millis).ok()?, rest))
}

fn number(text: &str) -> Option<(u32, &str)> {
    let end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    Some((text[..end].parse().ok()?, &text[end..]))
}
