use std::sync::LazyLock;

use reqlog::rollup::LATENCY_BUCKET_BOUNDS_MS;
use serde::Serialize;

pub const MEDIAN: f64 = 0.5;
pub const P95: f64 = 0.95;
pub const P99: f64 = 0.99;

const BUCKET_COUNT: usize = LATENCY_BUCKET_BOUNDS_MS.len() + 1;

pub static BUCKET_SUMS: LazyLock<String> = LazyLock::new(|| {
    let sums: Vec<String> = (1..=BUCKET_COUNT)
        .map(|bucket| format!("sum(latency_buckets[{bucket}])"))
        .collect();
    format!("ARRAY[{}]::bigint[]", sums.join(", "))
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    counts: [i64; BUCKET_COUNT],
    max_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Percentiles {
    pub p50: Option<i64>,
    pub p95: Option<i64>,
    pub p99: Option<i64>,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            counts: [0; BUCKET_COUNT],
            max_ms: 0,
        }
    }
}

impl Histogram {
    pub fn add(&mut self, counts: &[i64], max_ms: Option<i32>) {
        for (total, count) in self.counts.iter_mut().zip(counts) {
            *total += count;
        }
        self.max_ms = self.max_ms.max(max_ms.map_or(0, i64::from));
    }

    pub fn percentile(&self, quantile: f64) -> Option<i64> {
        let total: i64 = self.counts.iter().sum();
        if total == 0 {
            return None;
        }
        let rank = quantile * total as f64;
        let mut below = 0;
        for (bucket, &count) in self.counts.iter().enumerate() {
            if count > 0 && (below + count) as f64 >= rank {
                let (low, high) = self.bounds(bucket);
                let share = (rank - below as f64) / count as f64;
                return Some((low as f64 + (high - low) as f64 * share).round() as i64);
            }
            below += count;
        }
        Some(self.max_ms)
    }

    pub fn percentiles(&self) -> Percentiles {
        Percentiles {
            p50: self.percentile(MEDIAN),
            p95: self.percentile(P95),
            p99: self.percentile(P99),
        }
    }

    fn bounds(&self, bucket: usize) -> (i64, i64) {
        let low = match bucket {
            0 => 0,
            _ => i64::from(LATENCY_BUCKET_BOUNDS_MS[bucket - 1]),
        };
        let ceiling = LATENCY_BUCKET_BOUNDS_MS
            .get(bucket)
            .map_or(self.max_ms, |&bound| i64::from(bound));
        (low, ceiling.min(self.max_ms).max(low))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram(counts: [i64; BUCKET_COUNT], max_ms: i32) -> Histogram {
        let mut histogram = Histogram::default();
        histogram.add(&counts, Some(max_ms));
        histogram
    }

    #[test]
    fn an_empty_histogram_has_no_percentiles() {
        assert_eq!(Histogram::default().percentiles(), Percentiles::default());
    }

    #[test]
    fn percentiles_interpolate_inside_their_bucket() {
        let mut counts = [0; BUCKET_COUNT];
        counts[1] = 50;
        counts[3] = 50;
        let histogram = histogram(counts, 90);
        assert_eq!(histogram.percentile(MEDIAN), Some(25));
        assert_eq!(histogram.percentile(P95), Some(86));
    }

    #[test]
    fn the_open_last_bucket_ends_at_the_slowest_request() {
        let mut counts = [0; BUCKET_COUNT];
        counts[BUCKET_COUNT - 1] = 10;
        let histogram = histogram(counts, 42_000);
        assert_eq!(histogram.percentile(MEDIAN), Some(36_000));
        assert_eq!(histogram.percentile(1.0), Some(42_000));
    }

    #[test]
    fn a_bucket_never_reaches_past_the_slowest_request() {
        let mut counts = [0; BUCKET_COUNT];
        counts[4] = 4;
        assert_eq!(histogram(counts, 120).percentile(1.0), Some(120));
    }

    #[test]
    fn the_bucket_sums_cover_every_bucket() {
        assert_eq!(BUCKET_SUMS.matches("sum(").count(), BUCKET_COUNT);
        assert!(BUCKET_SUMS.contains(&format!("latency_buckets[{BUCKET_COUNT}]")));
    }
}
