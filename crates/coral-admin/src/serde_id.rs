use serde::Serializer;

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

    #[test]
    fn optional_none_serializes_as_null() {
        let value = WithOptId { id: None };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json, json!({ "id": null }));
    }
}
