use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serializer;

pub fn micros<S: Serializer>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format(value))
}

pub fn micros_opt<S: Serializer>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_str(&format(value)),
        None => serializer.serialize_none(),
    }
}

pub fn format(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn timestamps_always_carry_six_fractional_digits() {
        let whole = Utc.with_ymd_and_hms(2026, 10, 14, 3, 14, 32).unwrap();
        assert_eq!(format(&whole), "2026-10-14T03:14:32.000000Z");
        let fine = whole + chrono::Duration::microseconds(1);
        assert_eq!(format(&fine), "2026-10-14T03:14:32.000001Z");
    }
}
