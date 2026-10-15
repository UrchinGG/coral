use chrono::{DateTime, Utc};
use reqlog::rollup::{HOUR, Span, ceil_to, floor_to};
use reqlog::{Cause, KeyKind};
use sqlx::{Postgres, QueryBuilder};

use super::class::{RouteFilter, StatusClass};
use super::range::{Source, TrafficRange};

const ROW_COLUMNS: &str = "route, method, status, cause, requests, latency_max_ms, latency_buckets";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TrafficFilter {
    pub route: Option<RouteFilter>,
    pub method: Option<&'static str>,
    pub class: Option<StatusClass>,
    pub cause: Option<Cause>,
    pub key_kind: Option<KeyKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSpans {
    hours: Option<Span>,
    minutes: Vec<Span>,
}

impl RowSpans {
    pub fn minutes(span: Span) -> Self {
        Self {
            hours: None,
            minutes: vec![span],
        }
    }

    pub fn covering(range: TrafficRange) -> Self {
        match range.source {
            Source::Hours => Self {
                hours: Some(range.span),
                minutes: Vec::new(),
            },
            Source::Minutes => Self::whole_hours_and_edges(range.span),
        }
    }

    fn whole_hours_and_edges(span: Span) -> Self {
        let inner_from = ceil_to(span.from, HOUR);
        let inner_to = floor_to(span.to, HOUR);
        let Some(hours) = Span::new(inner_from, inner_to) else {
            return Self::minutes(span);
        };
        Self {
            hours: Some(hours),
            minutes: [
                Span::new(span.from, inner_from),
                Span::new(inner_to, span.to),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }
    }

    pub fn push_rows(&self, query: &mut QueryBuilder<'_, Postgres>, filter: &TrafficFilter) {
        let has_minutes = !self.minutes.is_empty();
        if let Some(hours) = self.hours {
            push_hour_rows(query, hours, filter);
            if has_minutes {
                query.push(" UNION ALL ");
            }
        }
        if has_minutes {
            push_minute_rows(query, &self.minutes, filter);
        }
    }
}

impl TrafficFilter {
    pub fn push_conditions(&self, query: &mut QueryBuilder<'_, Postgres>) {
        match self.route {
            Some(RouteFilter::Template(route)) => {
                query.push(" AND route = ").push_bind(route);
            }
            Some(RouteFilter::Unmatched) => {
                query.push(" AND route IS NULL");
            }
            None => {}
        }
        if let Some(method) = self.method {
            query.push(" AND method = ").push_bind(method);
        }
        if let Some(class) = self.class {
            query.push(format!(" AND {}", class.predicate()));
        }
        if let Some(cause) = self.cause {
            query.push(" AND cause = ").push_bind(cause.code());
        }
        if let Some(key_kind) = self.key_kind {
            query.push(" AND key_kind = ").push_bind(key_kind.code());
        }
    }
}

fn push_hour_rows(query: &mut QueryBuilder<'_, Postgres>, hours: Span, filter: &TrafficFilter) {
    query.push(format!(
        "SELECT hour AS t, {ROW_COLUMNS} FROM api_traffic_hour WHERE hour >= "
    ));
    push_bounds(query, "hour", hours.from, hours.to);
    filter.push_conditions(query);
}

fn push_minute_rows(
    query: &mut QueryBuilder<'_, Postgres>,
    spans: &[Span],
    filter: &TrafficFilter,
) {
    query.push(format!(
        "SELECT minute AS t, {ROW_COLUMNS} FROM api_traffic_minute WHERE ("
    ));
    for (index, span) in spans.iter().enumerate() {
        if index > 0 {
            query.push(" OR ");
        }
        query.push("(minute >= ");
        push_bounds(query, "minute", span.from, span.to);
        query.push(")");
    }
    query.push(")");
    filter.push_conditions(query);
}

fn push_bounds(
    query: &mut QueryBuilder<'_, Postgres>,
    column: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) {
    query
        .push_bind(from)
        .push(format!(" AND {column} < "))
        .push_bind(to);
}

#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone};

    use super::*;
    use crate::traffic::range::Step;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, hour, minute, 0).unwrap()
    }

    fn range(from: DateTime<Utc>, to: DateTime<Utc>, source: Source) -> TrafficRange {
        TrafficRange {
            span: Span::new(from, to).unwrap(),
            step: Step::FiveMinutes,
            source,
        }
    }

    fn sql(spans: &RowSpans, filter: &TrafficFilter) -> String {
        let mut query = QueryBuilder::new("");
        spans.push_rows(&mut query, filter);
        query.into_sql()
    }

    #[test]
    fn whole_hours_come_from_the_hour_table_and_edges_from_minutes() {
        let spans = RowSpans::covering(range(at(6, 40), at(9, 20), Source::Minutes));
        assert_eq!(spans.hours, Span::new(at(7, 0), at(9, 0)));
        assert_eq!(
            spans.minutes,
            vec![
                Span::new(at(6, 40), at(7, 0)).unwrap(),
                Span::new(at(9, 0), at(9, 20)).unwrap()
            ]
        );
    }

    #[test]
    fn a_range_inside_one_hour_reads_only_minutes() {
        let span = Span::new(at(6, 10), at(6, 50)).unwrap();
        let spans = RowSpans::covering(range(span.from, span.to, Source::Minutes));
        assert_eq!(spans, RowSpans::minutes(span));
    }

    #[test]
    fn hour_aligned_ranges_need_no_minute_edges() {
        let aligned = RowSpans::covering(range(at(6, 0), at(8, 0), Source::Minutes));
        assert!(aligned.minutes.is_empty());
        let old = RowSpans::covering(range(
            at(1, 0),
            at(5, 0) + TimeDelta::hours(1),
            Source::Hours,
        ));
        assert!(old.minutes.is_empty());
        assert!(!sql(&old, &TrafficFilter::default()).contains("UNION ALL"));
    }

    #[test]
    fn filters_apply_to_every_branch() {
        let spans = RowSpans::covering(range(at(6, 40), at(9, 20), Source::Minutes));
        let filter = TrafficFilter {
            route: Some(RouteFilter::Unmatched),
            class: Some(StatusClass::RateLimited),
            cause: Some(Cause::CallerLimit),
            ..TrafficFilter::default()
        };
        let text = sql(&spans, &filter);
        assert_eq!(text.matches("route IS NULL").count(), 2);
        assert_eq!(text.matches("status = 429").count(), 2);
        assert_eq!(text.matches("cause = $").count(), 2);
        assert_eq!(text.matches("UNION ALL").count(), 1);
        assert_eq!(text.matches("(minute >= ").count(), 2);
    }
}
