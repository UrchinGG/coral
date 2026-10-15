use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use serde::Deserialize;

use crate::error::AdminError;
use crate::names::LabelRequest;
use crate::query::PlayerInput;
use crate::refs::Labels;
use crate::state::AppState;

pub const MAX_LABEL_IDS: usize = 200;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelsBody {
    #[serde(default)]
    discord: Vec<String>,
    #[serde(default)]
    minecraft: Vec<String>,
}

pub async fn labels(
    State(state): State<AppState>,
    body: Result<Json<LabelsBody>, JsonRejection>,
) -> Result<Json<Labels>, AdminError> {
    let Json(body) = body?;
    let request = body.into_request()?;
    Ok(Json(state.names.resolve(&request).await?))
}

impl LabelsBody {
    fn into_request(self) -> Result<LabelRequest, AdminError> {
        if self.discord.len() > MAX_LABEL_IDS || self.minecraft.len() > MAX_LABEL_IDS {
            return Err(AdminError::BadRequest(format!(
                "at most {MAX_LABEL_IDS} ids of each kind per request"
            )));
        }
        Ok(LabelRequest {
            discord: self
                .discord
                .iter()
                .map(|raw| discord_id(raw))
                .collect::<Result<_, _>>()?,
            minecraft: self
                .minecraft
                .iter()
                .map(|raw| player_uuid(raw))
                .collect::<Result<_, _>>()?,
        })
    }
}

fn discord_id(raw: &str) -> Result<i64, AdminError> {
    raw.trim()
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a Discord id")))
}

fn player_uuid(raw: &str) -> Result<String, AdminError> {
    match PlayerInput::parse(raw)? {
        PlayerInput::Uuid(uuid) => Ok(uuid.simple().to_string()),
        PlayerInput::Name(_) => Err(AdminError::BadRequest(format!(
            "{raw:?} is not a player UUID"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request(body: serde_json::Value) -> Result<LabelRequest, AdminError> {
        serde_json::from_value::<LabelsBody>(body)
            .unwrap()
            .into_request()
    }

    #[test]
    fn ids_are_parsed_and_uuids_normalized() {
        let parsed = request(json!({
            "discord": ["354598337325826048"],
            "minecraft": ["94589C87-999B-4A7A-B3AD-D2AE50711E64"],
        }))
        .unwrap();
        assert_eq!(parsed.discord, vec![354_598_337_325_826_048]);
        assert_eq!(
            parsed.minecraft,
            vec!["94589c87999b4a7ab3add2ae50711e64".to_string()]
        );
    }

    #[test]
    fn a_body_that_does_not_parse_is_a_bad_request() {
        let rejection = Json::<LabelsBody>::from_bytes(br#"{"bogus": 1}"#).unwrap_err();
        assert_eq!(AdminError::from(rejection).code(), "bad_request");
    }

    #[test]
    fn either_list_may_be_omitted() {
        assert_eq!(request(json!({})).unwrap(), LabelRequest::default());
    }

    #[test]
    fn malformed_ids_and_oversized_batches_are_rejected() {
        let too_many: Vec<String> = (1..=MAX_LABEL_IDS as i64 + 1)
            .map(|id| id.to_string())
            .collect();
        for body in [
            json!({ "discord": ["abc"] }),
            json!({ "discord": ["-5"] }),
            json!({ "minecraft": ["oJugg"] }),
            json!({ "discord": too_many }),
        ] {
            assert_eq!(
                request(body.clone()).unwrap_err().code(),
                "bad_request",
                "{body}"
            );
        }
    }
}
