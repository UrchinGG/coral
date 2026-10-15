use serde::{Deserialize, Deserializer};

use super::forms::{FormSelection, PlayerWindow};
use super::requester::RequesterKey;
use crate::error::AdminError;
use crate::query::Cursor;

const FLAG_ON: [&str; 2] = ["1", "true"];
const FLAG_OFF: [&str; 3] = ["", "0", "false"];

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestersParams {
    #[serde(default)]
    pub window: PlayerWindow,
    pub forms: Option<String>,
    #[serde(default, deserialize_with = "flag")]
    pub include_display: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamParams {
    #[serde(default)]
    pub window: PlayerWindow,
    pub forms: Option<String>,
    #[serde(default, deserialize_with = "flag")]
    pub include_display: bool,
    pub requester: Option<String>,
    pub cursor: Option<Cursor>,
}

impl RequestersParams {
    pub fn forms(&self) -> Result<FormSelection, AdminError> {
        FormSelection::parse(self.forms.as_deref(), self.include_display)
    }
}

impl StreamParams {
    pub fn forms(&self) -> Result<FormSelection, AdminError> {
        FormSelection::parse(self.forms.as_deref(), self.include_display)
    }

    pub fn requester(&self) -> Result<Option<RequesterKey>, AdminError> {
        self.requester
            .as_deref()
            .map(RequesterKey::decode)
            .transpose()
    }
}

fn flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let value = raw.trim();
    if FLAG_ON.contains(&value) {
        return Ok(true);
    }
    if FLAG_OFF.contains(&value) {
        return Ok(false);
    }
    Err(serde::de::Error::custom(format!(
        "{raw:?} is not a flag; use 1 or 0"
    )))
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;

    use super::*;
    use crate::players::forms::PlayerForm;

    fn stream(uri: &'static str) -> Result<StreamParams, AdminError> {
        let Query(params) = Query::<StreamParams>::try_from_uri(&Uri::from_static(uri))?;
        Ok(params)
    }

    #[test]
    fn defaults_are_fourteen_days_without_display_names() {
        let params = stream("/players/x/requests").unwrap();
        assert_eq!(params.window, PlayerWindow::Fortnight);
        assert_eq!(params.forms().unwrap(), FormSelection::standard());
        assert_eq!(params.requester().unwrap(), None);
    }

    #[test]
    fn windows_forms_and_the_display_flag_parse() {
        let params = stream("/p?window=24h&forms=body,former&include_display=1").unwrap();
        assert_eq!(params.window, PlayerWindow::Day);
        let forms = params.forms().unwrap();
        assert!(forms.contains(PlayerForm::Body));
        assert!(forms.contains(PlayerForm::Display));
        assert!(!forms.contains(PlayerForm::Undashed));
    }

    #[test]
    fn malformed_parameters_are_bad_requests() {
        for uri in [
            "/p?window=30d",
            "/p?include_display=yes",
            "/p?cursor=nope",
            "/p?unknown=1",
        ] {
            assert_eq!(stream(uri).unwrap_err().code(), "bad_request", "{uri}");
        }
        let params = stream("/p?requester=nope").unwrap();
        assert_eq!(params.requester().unwrap_err().code(), "bad_request");
    }
}
