use serde_json::{Number, Value};

const LARGEST_EXACT_SCRIPT_INTEGER: u64 = (1 << 53) - 1;

pub fn exact_integers(value: Value) -> Value {
    match value {
        Value::Number(number) if !exact_in_script(&number) => Value::String(number.to_string()),
        Value::Array(items) => Value::Array(items.into_iter().map(exact_integers).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(name, field)| (name, exact_integers(field)))
                .collect(),
        ),
        other => other,
    }
}

fn exact_in_script(number: &Number) -> bool {
    let magnitude = number
        .as_u64()
        .or_else(|| number.as_i64().map(i64::unsigned_abs));
    magnitude.is_none_or(|magnitude| magnitude <= LARGEST_EXACT_SCRIPT_INTEGER)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn snowflakes_become_strings_wherever_they_sit_and_small_numbers_stay() {
        let row = json!({
            "id": 21111,
            "discord_id": 354_598_337_325_826_048_i64,
            "strikes": [{ "struck_by": 1_218_840_276_405_911_602_i64, "strike_id": 4 }],
            "config": { "pause_name_updates": true, "ratio": 0.5, "offset": -9_007_199_254_740_993_i64 },
        });
        assert_eq!(
            exact_integers(row),
            json!({
                "id": 21111,
                "discord_id": "354598337325826048",
                "strikes": [{ "struck_by": "1218840276405911602", "strike_id": 4 }],
                "config": { "pause_name_updates": true, "ratio": 0.5, "offset": "-9007199254740993" },
            })
        );
    }

    #[test]
    fn the_largest_exact_integer_stays_a_number() {
        let largest = json!(LARGEST_EXACT_SCRIPT_INTEGER);
        assert_eq!(exact_integers(largest.clone()), largest);
        assert_eq!(
            exact_integers(json!(LARGEST_EXACT_SCRIPT_INTEGER + 1)),
            json!("9007199254740992")
        );
    }
}
