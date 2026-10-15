use chrono::{DateTime, TimeDelta, Utc};
use reqlog::rollup::{HOUR, MINUTE, Span, TRAFFIC_MINUTE_RETENTION_DAYS, ceil_to};
use serde::{Deserialize, Serialize};

use crate::error::AdminError;
use crate::serde_ts;

pub const MAX_CUSTOM_SPAN: TimeDelta = TimeDelta::days(31);

const FIVE_MINUTES: TimeDelta = TimeDelta::minutes(5);
const MINUTE_STEP_LIMIT: TimeDelta = TimeDelta::hours(6);
const MINUTE_TABLE_LIMIT: TimeDelta = TimeDelta::hours(48);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Window {
    #[serde(rename = "1h")]
    Hour,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Minute,
    FiveMinutes,
    Hour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Minutes,
    Hours,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RangeQuery {
    pub window: Option<Window>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrafficRange {
    pub span: Span,
    pub step: Step,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RangeBounds {
    #[serde(serialize_with = "serde_ts::micros")]
    pub from: DateTime<Utc>,
    #[serde(serialize_with = "serde_ts::micros")]
    pub to: DateTime<Utc>,
}

impl Window {
    pub fn length(self) -> TimeDelta {
        match self {
            Self::Hour => TimeDelta::hours(1),
            Self::SixHours => TimeDelta::hours(6),
            Self::Day => TimeDelta::days(1),
            Self::Week => TimeDelta::days(7),
            Self::Month => TimeDelta::days(30),
        }
    }
}

impl Step {
    pub fn for_length(length: TimeDelta) -> Self {
        match length {
            length if length <= MINUTE_STEP_LIMIT => Self::Minute,
            length if length <= MINUTE_TABLE_LIMIT => Self::FiveMinutes,
            _ => Self::Hour,
        }
    }

    pub fn delta(self) -> TimeDelta {
        match self {
            Self::Minute => MINUTE,
            Self::FiveMinutes => FIVE_MINUTES,
            Self::Hour => HOUR,
        }
    }

    pub fn seconds(self) -> i64 {
        self.delta().num_seconds()
    }
}

impl RangeQuery {
    pub fn resolve(self, now: DateTime<Utc>) -> Result<TrafficRange, AdminError> {
        match (self.window, self.from, self.to) {
            (Some(window), None, None) => Ok(TrafficRange::ending_at(now, window.length())),
            (None, Some(from), Some(to)) => TrafficRange::between(from, to, now),
            _ => Err(AdminError::BadRequest(
                "pass either window, or both from and to".into(),
            )),
        }
    }

    pub fn cache_key(self, range: TrafficRange) -> String {
        match self.window {
            Some(window) => format!("{window:?}"),
            None => format!(
                "{}..{}",
                range.span.from.timestamp(),
                range.span.to.timestamp()
            ),
        }
    }
}

impl TrafficRange {
    pub fn ending_at(now: DateTime<Utc>, length: TimeDelta) -> Self {
        let step = Step::for_length(length);
        let to = ceil_to(now, step.delta());
        Self::stepped(
            Span {
                from: to - length,
                to,
            },
            step,
            now,
        )
    }

    pub fn between(
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, AdminError> {
        let span = Span::new(from, to)
            .ok_or_else(|| AdminError::BadRequest("from must be before to".into()))?;
        if to - from > MAX_CUSTOM_SPAN {
            return Err(AdminError::BadRequest(format!(
                "a traffic range can span at most {} days",
                MAX_CUSTOM_SPAN.num_days()
            )));
        }
        let step = Step::for_length(to - from);
        Ok(Self::stepped(span.aligned(step.delta()), step, now))
    }

    pub fn previous(self, now: DateTime<Utc>) -> Self {
        let span = Span {
            from: self.span.from - self.length(),
            to: self.span.from,
        };
        Self::stepped(span, self.step, now)
    }

    pub fn hourly(self) -> Self {
        Self {
            span: self.span.aligned(HOUR),
            step: Step::Hour,
            source: Source::Hours,
        }
    }

    pub fn length(self) -> TimeDelta {
        self.span.to - self.span.from
    }

    pub fn bounds(self) -> RangeBounds {
        RangeBounds {
            from: self.span.from,
            to: self.span.to,
        }
    }

    pub fn buckets(self) -> impl Iterator<Item = DateTime<Utc>> {
        let step = self.step.delta();
        let to = self.span.to;
        std::iter::successors(Some(self.span.from), move |at| Some(*at + step))
            .take_while(move |at| *at < to)
    }

    pub fn bucket_count(self) -> i64 {
        self.length().num_seconds() / self.step.seconds()
    }

    fn stepped(span: Span, step: Step, now: DateTime<Utc>) -> Self {
        let minutes_kept_from = now - TimeDelta::days(TRAFFIC_MINUTE_RETENTION_DAYS);
        let minutes_cover = step != Step::Hour && span.from >= minutes_kept_from;
        match minutes_cover {
            true => Self {
                span,
                step,
                source: Source::Minutes,
            },
            false => Self {
                span: span.aligned(HOUR),
                step: Step::Hour,
                source: Source::Hours,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    fn now() -> DateTime<Utc> {
        at(15, 12, 34) + TimeDelta::seconds(56)
    }

    fn window(window: Window) -> TrafficRange {
        RangeQuery {
            window: Some(window),
            ..RangeQuery::default()
        }
        .resolve(now())
        .unwrap()
    }

    #[test]
    fn windows_end_at_the_step_holding_now_and_pick_their_step_and_table() {
        let hour = window(Window::Hour);
        assert_eq!(hour.span.to, at(15, 12, 35));
        assert_eq!(hour.span.from, at(15, 11, 35));
        assert_eq!((hour.step, hour.source), (Step::Minute, Source::Minutes));
        assert_eq!(hour.buckets().count(), 60);
        assert_eq!(hour.bucket_count(), 60);

        let day = window(Window::Day);
        assert_eq!(day.span.to, at(15, 12, 35));
        assert_eq!((day.step, day.source), (Step::FiveMinutes, Source::Minutes));
        assert_eq!(day.buckets().count(), 288);

        let week = window(Window::Week);
        assert_eq!(week.span.to, at(15, 13, 0));
        assert_eq!((week.step, week.source), (Step::Hour, Source::Hours));
        assert_eq!(week.buckets().count(), 168);
    }

    #[test]
    fn custom_ranges_snap_outward_to_their_step() {
        let range =
            TrafficRange::between(at(15, 10, 2) + TimeDelta::seconds(9), at(15, 11, 58), now())
                .unwrap();
        assert_eq!(range.span.from, at(15, 10, 2));
        assert_eq!(range.span.to, at(15, 11, 58));
        let two_days = TrafficRange::between(at(13, 10, 3), at(15, 10, 1), now()).unwrap();
        assert_eq!(two_days.step, Step::FiveMinutes);
        assert_eq!(two_days.span.from, at(13, 10, 0));
        assert_eq!(two_days.span.to, at(15, 10, 5));
    }

    #[test]
    fn ranges_older_than_the_minute_table_read_whole_hours() {
        let old = TrafficRange::between(at(1, 10, 10), at(1, 11, 20), at(31, 12, 0)).unwrap();
        assert_eq!((old.step, old.source), (Step::Hour, Source::Hours));
        assert_eq!(old.span.from, at(1, 10, 0));
        assert_eq!(old.span.to, at(1, 12, 0));
    }

    #[test]
    fn a_range_is_a_window_or_both_bounds_and_never_longer_than_the_cap() {
        let both = RangeQuery {
            window: Some(Window::Hour),
            from: Some(at(15, 10, 0)),
            to: Some(at(15, 11, 0)),
        };
        assert_eq!(both.resolve(now()).unwrap_err().code(), "bad_request");
        let half = RangeQuery {
            from: Some(at(15, 10, 0)),
            ..RangeQuery::default()
        };
        assert_eq!(half.resolve(now()).unwrap_err().code(), "bad_request");
        let backwards = TrafficRange::between(at(15, 11, 0), at(15, 10, 0), now());
        assert_eq!(backwards.unwrap_err().code(), "bad_request");
        let long = TrafficRange::between(at(1, 0, 0), at(1, 0, 0) + TimeDelta::days(32), now());
        assert_eq!(long.unwrap_err().code(), "bad_request");
    }

    #[test]
    fn the_previous_range_is_the_same_length_just_before() {
        let day = window(Window::Day);
        let previous = day.previous(now());
        assert_eq!(previous.span.to, day.span.from);
        assert_eq!(previous.length(), day.length());
    }

    #[test]
    fn hourly_ranges_widen_to_whole_hours() {
        let range = TrafficRange::between(at(15, 9, 40), at(15, 12, 20), now()).unwrap();
        let hourly = range.hourly();
        assert_eq!(hourly.span.from, at(15, 9, 0));
        assert_eq!(hourly.span.to, at(15, 13, 0));
        assert_eq!((hourly.step, hourly.source), (Step::Hour, Source::Hours));
    }

    #[test]
    fn cache_keys_name_the_window_or_the_aligned_bounds() {
        let query = RangeQuery {
            window: Some(Window::Day),
            ..RangeQuery::default()
        };
        assert_eq!(query.cache_key(window(Window::Day)), "Day");
        let custom = RangeQuery {
            from: Some(at(15, 10, 0)),
            to: Some(at(15, 11, 0)),
            ..RangeQuery::default()
        };
        let range = custom.resolve(now()).unwrap();
        assert_eq!(
            custom.cache_key(range),
            format!(
                "{}..{}",
                at(15, 10, 0).timestamp(),
                at(15, 11, 0).timestamp()
            )
        );
    }
}
