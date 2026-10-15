use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serializer;

pub fn micros<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_rfc3339_opts(SecondsFormat::Micros, true))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct Stamped {
        #[serde(serialize_with = "micros")]
        at: DateTime<Utc>,
    }

    fn serialized(at: DateTime<Utc>) -> String {
        serde_json::to_value(Stamped { at }).unwrap()["at"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn timestamps_always_carry_six_fractional_digits() {
        let whole = Utc.with_ymd_and_hms(2026, 10, 14, 3, 14, 32).unwrap();
        assert_eq!(serialized(whole), "2026-10-14T03:14:32.000000Z");
        let fine = whole + chrono::Duration::nanoseconds(1_234_567);
        assert_eq!(serialized(fine), "2026-10-14T03:14:32.001234Z");
    }
}
