use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use serde::Deserialize;

const HOUR: TimeDelta = TimeDelta::hours(1);
const DAY: TimeDelta = TimeDelta::days(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ActivityWindow {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "400d")]
    Retention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum LookupWindow {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "14d")]
    Fortnight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub step: TimeDelta,
}

impl ActivityWindow {
    pub fn span(self, now: DateTime<Utc>) -> Span {
        let (length, step) = match self {
            Self::Day => (TimeDelta::hours(24), HOUR),
            Self::Week => (TimeDelta::days(7), HOUR),
            Self::Month => (TimeDelta::days(30), DAY),
            Self::Retention => (TimeDelta::days(400), DAY),
        };
        let to = now.duration_trunc(step).map_or(now, |start| start + step);
        Span {
            from: to - length,
            to,
            step,
        }
    }
}

impl LookupWindow {
    pub fn length(self) -> TimeDelta {
        match self {
            Self::Day => TimeDelta::hours(24),
            Self::Fortnight => TimeDelta::days(14),
        }
    }

    pub fn from(self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - self.length()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn activity_spans_end_after_the_current_bucket() {
        let now = Utc.with_ymd_and_hms(2026, 10, 15, 4, 47, 11).unwrap();
        let day = ActivityWindow::Day.span(now);
        assert_eq!(day.to, Utc.with_ymd_and_hms(2026, 10, 15, 5, 0, 0).unwrap());
        assert_eq!(
            day.from,
            Utc.with_ymd_and_hms(2026, 10, 14, 5, 0, 0).unwrap()
        );
        assert_eq!(day.step, HOUR);
        let month = ActivityWindow::Month.span(now);
        assert_eq!(
            month.to,
            Utc.with_ymd_and_hms(2026, 10, 16, 0, 0, 0).unwrap()
        );
        assert_eq!(month.step, DAY);
    }
}
