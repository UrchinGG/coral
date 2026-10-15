use chrono::TimeDelta;

use super::{CALLER_RETENTION_DAYS, TRAFFIC_HOUR_RETENTION_DAYS, TRAFFIC_MINUTE_RETENTION_DAYS};

#[derive(Debug)]
pub struct RollupTable {
    pub name: &'static str,
    pub constraint: &'static str,
    pub time: &'static str,
    pub keys: &'static [&'static str],
    pub values: &'static [&'static str],
    pub retention_days: i64,
}

const TRAFFIC_KEYS: &[&str] = &["route", "method", "status", "cause", "key_kind"];

const TRAFFIC_VALUES: &[&str] = &[
    "requests",
    "latency_sum_ms",
    "latency_max_ms",
    "latency_buckets",
];

pub const TRAFFIC_MINUTE: RollupTable = RollupTable {
    name: "api_traffic_minute",
    constraint: "api_traffic_minute_key",
    time: "minute",
    keys: TRAFFIC_KEYS,
    values: TRAFFIC_VALUES,
    retention_days: TRAFFIC_MINUTE_RETENTION_DAYS,
};

pub const TRAFFIC_HOUR: RollupTable = RollupTable {
    name: "api_traffic_hour",
    constraint: "api_traffic_hour_key",
    time: "hour",
    keys: TRAFFIC_KEYS,
    values: TRAFFIC_VALUES,
    retention_days: TRAFFIC_HOUR_RETENTION_DAYS,
};

pub const CALLER_HOUR: RollupTable = RollupTable {
    name: "api_caller_hour",
    constraint: "api_caller_hour_key",
    time: "hour",
    keys: &[
        "key_kind",
        "member_id",
        "actor_id",
        "key_prefix",
        "service",
        "route",
        "method",
        "status",
        "cause",
    ],
    values: &["requests", "latency_sum_ms", "subjects"],
    retention_days: CALLER_RETENTION_DAYS,
};

pub const ROLLUP_TABLES: [&RollupTable; 3] = [&TRAFFIC_MINUTE, &TRAFFIC_HOUR, &CALLER_HOUR];

impl RollupTable {
    pub fn retention(&self) -> TimeDelta {
        TimeDelta::days(self.retention_days)
    }

    pub fn rewrite(&self, sources: &str) -> String {
        let table = self.name;
        let time = self.time;
        let columns = self.columns().join(", ");
        format!(
            "WITH {sources},
compared AS (
  SELECT s.*,
         count(*) OVER (PARTITION BY {columns}) AS identical,
         count(*) FILTER (WHERE s.stored_row IS NULL) OVER (PARTITION BY {row_key}) AS produced_for_key
  FROM (
    SELECT NULL::tid AS stored_row, {columns} FROM produced
    UNION ALL
    SELECT ctid, {columns} FROM {table} WHERE {time} >= $1 AND {time} < $2
  ) s
),
written AS (
  INSERT INTO {table} ({columns})
  SELECT {columns} FROM compared WHERE stored_row IS NULL AND identical = 1
  ON CONFLICT ON CONSTRAINT {constraint} DO UPDATE SET {assignments}
  RETURNING 1
),
removed AS (
  DELETE FROM {table}
  WHERE ctid = ANY (ARRAY(
    SELECT stored_row FROM compared WHERE stored_row IS NOT NULL AND produced_for_key = 0))
  RETURNING 1
)
SELECT (SELECT count(*) FROM written) AS written, (SELECT count(*) FROM removed) AS removed",
            row_key = self.row_key().join(", "),
            constraint = self.constraint,
            assignments = self.assignments(),
        )
    }

    pub fn prune(&self) -> String {
        format!(
            "DELETE FROM {table} WHERE ctid = ANY (ARRAY(
               SELECT ctid FROM {table} WHERE {time} < $1 LIMIT $2))",
            table = self.name,
            time = self.time,
        )
    }

    fn row_key(&self) -> Vec<&'static str> {
        std::iter::once(self.time)
            .chain(self.keys.iter().copied())
            .collect()
    }

    fn columns(&self) -> Vec<&'static str> {
        self.row_key()
            .into_iter()
            .chain(self.values.iter().copied())
            .collect()
    }

    fn assignments(&self) -> String {
        self.values
            .iter()
            .map(|value| format!("{value} = excluded.{value}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_list_every_column_of_the_table_once() {
        assert_eq!(
            CALLER_HOUR.columns().join(", "),
            "hour, key_kind, member_id, actor_id, key_prefix, service, route, method, status, \
             cause, requests, latency_sum_ms, subjects"
        );
    }

    #[test]
    fn stored_and_produced_rows_meet_only_by_grouping_never_by_a_join() {
        let rewrite = TRAFFIC_MINUTE.rewrite("produced AS (SELECT 1)");
        assert!(rewrite.contains(
            "count(*) FILTER (WHERE s.stored_row IS NULL) OVER (PARTITION BY minute, route, \
             method, status, cause, key_kind) AS produced_for_key"
        ));
        assert!(!rewrite.contains("JOIN") && !rewrite.contains("EXISTS"));
    }
}
