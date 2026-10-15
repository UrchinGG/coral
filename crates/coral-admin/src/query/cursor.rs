use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::AdminError;

const MICROS_BYTES: usize = 8;
const ENCODED_BYTES: usize = MICROS_BYTES + 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub ts: DateTime<Utc>,
    pub request_id: Uuid,
}

impl Cursor {
    pub fn new(ts: DateTime<Utc>, request_id: Uuid) -> Self {
        Self { ts, request_id }
    }

    pub fn before(ts: DateTime<Utc>) -> Self {
        Self::new(ts, Uuid::nil())
    }

    pub fn encode(&self) -> String {
        let mut bytes = [0u8; ENCODED_BYTES];
        bytes[..MICROS_BYTES].copy_from_slice(&self.ts.timestamp_micros().to_be_bytes());
        bytes[MICROS_BYTES..].copy_from_slice(self.request_id.as_bytes());
        URL_SAFE_NO_PAD.encode(bytes)
    }

    pub fn decode(raw: &str) -> Result<Self, AdminError> {
        let invalid = || AdminError::BadRequest(format!("cursor {raw:?} is not a valid cursor"));
        let bytes: [u8; ENCODED_BYTES] = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| invalid())?
            .try_into()
            .map_err(|_| invalid())?;
        let (micros, request_id) = bytes.split_at(MICROS_BYTES);
        let micros = i64::from_be_bytes(micros.try_into().map_err(|_| invalid())?);
        let ts = DateTime::from_timestamp_micros(micros).ok_or_else(invalid)?;
        let request_id = Uuid::from_slice(request_id).map_err(|_| invalid())?;
        Ok(Self::new(ts, request_id))
    }
}

impl Serialize for Cursor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for Cursor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::decode(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn sample() -> Cursor {
        let ts = Utc.with_ymd_and_hms(2026, 10, 14, 21, 41, 7).unwrap()
            + chrono::Duration::microseconds(556_439);
        Cursor::new(
            ts,
            Uuid::parse_str("0199e4a1-7c2b-7d10-8a3e-2f1d9c4b5a60").unwrap(),
        )
    }

    #[test]
    fn cursors_round_trip_with_microsecond_precision() {
        let cursor = sample();
        assert_eq!(Cursor::decode(&cursor.encode()).unwrap(), cursor);
    }

    #[test]
    fn encoding_is_url_safe_and_unpadded() {
        let encoded = sample().encode();
        assert_eq!(encoded.len(), 32);
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn cursor_order_matches_the_page_order() {
        let earlier = Cursor::new(sample().ts - chrono::Duration::microseconds(1), Uuid::max());
        assert!(earlier < sample());
        assert!(Cursor::before(sample().ts) < sample());
    }

    #[test]
    fn malformed_cursors_are_bad_requests() {
        for raw in [
            "",
            "not base64!",
            "AAAA",
            &format!("{}A", sample().encode()),
        ] {
            let error = Cursor::decode(raw).unwrap_err();
            assert_eq!(error.code(), "bad_request", "{raw:?}");
        }
    }

    #[test]
    fn cursors_travel_as_json_strings() {
        let cursor = sample();
        let json = serde_json::to_string(&cursor).unwrap();
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cursor);
        assert!(serde_json::from_str::<Cursor>("\"bogus\"").is_err());
    }
}
