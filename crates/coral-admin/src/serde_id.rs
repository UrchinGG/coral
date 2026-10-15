use serde::Serializer;
use serde::ser::SerializeSeq;
use uuid::Uuid;

pub fn discord_id<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

pub fn discord_id_opt<S: Serializer>(
    value: &Option<i64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) => serializer.collect_str(v),
        None => serializer.serialize_none(),
    }
}

pub fn discord_ids<S: Serializer>(values: &[i64], serializer: S) -> Result<S::Ok, S::Error> {
    let mut seq = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        seq.serialize_element(&value.to_string())?;
    }
    seq.end()
}

pub fn uuid_undashed<S: Serializer>(value: &Uuid, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&value.simple())
}

pub fn uuid_undashed_opt<S: Serializer>(
    value: &Option<Uuid>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) => serializer.collect_str(&v.simple()),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    struct WithId {
        #[serde(serialize_with = "discord_id")]
        id: i64,
    }

    #[derive(Serialize)]
    struct WithOptId {
        #[serde(serialize_with = "discord_id_opt")]
        id: Option<i64>,
    }

    #[test]
    fn serializes_snowflake_as_string_not_lossy_number() {
        let beyond_js_safe_integer = 1_547_000_000_000_000_000_i64;
        let value = WithId {
            id: beyond_js_safe_integer,
        };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json, json!({ "id": "1547000000000000000" }));
    }

    #[test]
    fn optional_some_serializes_as_string() {
        let value = WithOptId {
            id: Some(354598337325826048),
        };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json, json!({ "id": "354598337325826048" }));
    }

    #[derive(Serialize)]
    struct WithIds {
        #[serde(serialize_with = "discord_ids")]
        ids: Vec<i64>,
        #[serde(serialize_with = "uuid_undashed")]
        player: Uuid,
        #[serde(serialize_with = "uuid_undashed_opt")]
        other: Option<Uuid>,
    }

    #[test]
    fn id_lists_and_uuids_use_their_wire_forms() {
        let value = WithIds {
            ids: vec![1_547_000_000_000_000_000],
            player: Uuid::parse_str("94589c87-999b-4a7a-b3ad-d2ae50711e64").unwrap(),
            other: None,
        };
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            json!({
                "ids": ["1547000000000000000"],
                "player": "94589c87999b4a7ab3add2ae50711e64",
                "other": null,
            })
        );
    }

    #[test]
    fn optional_none_serializes_as_null() {
        let value = WithOptId { id: None };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json, json!({ "id": null }));
    }
}
