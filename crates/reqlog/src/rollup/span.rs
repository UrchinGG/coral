use chrono::{DateTime, DurationRound, TimeDelta, Utc};

pub const MINUTE: TimeDelta = TimeDelta::minutes(1);
pub const HOUR: TimeDelta = TimeDelta::hours(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl Span {
    pub fn new(from: DateTime<Utc>, to: DateTime<Utc>) -> Option<Self> {
        (from < to).then_some(Self { from, to })
    }

    pub fn aligned(self, unit: TimeDelta) -> Self {
        Self {
            from: floor_to(self.from, unit),
            to: ceil_to(self.to, unit),
        }
    }

    pub fn starting_no_earlier_than(self, floor: DateTime<Utc>) -> Option<Self> {
        Self::new(self.from.max(floor), self.to)
    }

    pub fn joined(self, other: Self) -> Self {
        Self {
            from: self.from.min(other.from),
            to: self.to.max(other.to),
        }
    }

    pub fn touches(self, other: Self) -> bool {
        self.from <= other.to && other.from <= self.to
    }

    pub fn chunks(self, size: TimeDelta) -> impl Iterator<Item = Self> {
        let first = Self::new(self.from, (self.from + size).min(self.to));
        std::iter::successors(first, move |chunk| {
            Self::new(chunk.to, (chunk.to + size).min(self.to))
        })
    }
}

pub fn floor_to(at: DateTime<Utc>, unit: TimeDelta) -> DateTime<Utc> {
    at.duration_trunc(unit)
        .expect("minute and hour boundaries of a stored timestamp are representable")
}

pub fn ceil_to(at: DateTime<Utc>, unit: TimeDelta) -> DateTime<Utc> {
    let floor = floor_to(at, unit);
    match floor == at {
        true => at,
        false => floor + unit,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, second)
            .unwrap()
    }

    #[test]
    fn empty_and_inverted_spans_do_not_exist() {
        assert_eq!(Span::new(at(1, 0, 0), at(1, 0, 0)), None);
        assert_eq!(Span::new(at(2, 0, 0), at(1, 0, 0)), None);
    }

    #[test]
    fn alignment_widens_to_whole_units() {
        let span = Span::new(at(10, 20, 5), at(11, 40, 0)).unwrap();
        assert_eq!(
            span.aligned(HOUR),
            Span::new(at(10, 0, 0), at(12, 0, 0)).unwrap()
        );
        assert_eq!(
            span.aligned(MINUTE),
            Span::new(at(10, 20, 0), at(11, 40, 0)).unwrap()
        );
    }

    #[test]
    fn chunks_cover_the_span_without_gaps() {
        let span = Span::new(at(10, 30, 0), at(13, 0, 0)).unwrap();
        let chunks: Vec<Span> = span.chunks(HOUR).collect();
        assert_eq!(
            chunks,
            vec![
                Span::new(at(10, 30, 0), at(11, 30, 0)).unwrap(),
                Span::new(at(11, 30, 0), at(12, 30, 0)).unwrap(),
                Span::new(at(12, 30, 0), at(13, 0, 0)).unwrap(),
            ]
        );
    }

    #[test]
    fn a_floor_trims_the_start_or_removes_the_span() {
        let span = Span::new(at(10, 0, 0), at(12, 0, 0)).unwrap();
        assert_eq!(
            span.starting_no_earlier_than(at(11, 0, 0)),
            Span::new(at(11, 0, 0), at(12, 0, 0))
        );
        assert_eq!(span.starting_no_earlier_than(at(9, 0, 0)), Some(span));
        assert_eq!(span.starting_no_earlier_than(at(12, 0, 0)), None);
    }

    #[test]
    fn touching_spans_join_into_one() {
        let first = Span::new(at(10, 0, 0), at(11, 0, 0)).unwrap();
        let adjacent = Span::new(at(11, 0, 0), at(12, 0, 0)).unwrap();
        let apart = Span::new(at(13, 0, 0), at(14, 0, 0)).unwrap();
        assert!(first.touches(adjacent));
        assert!(!first.touches(apart));
        assert_eq!(
            first.joined(adjacent),
            Span::new(at(10, 0, 0), at(12, 0, 0)).unwrap()
        );
    }
}
