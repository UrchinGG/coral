use reqlog::Form;
use serde::Serialize;
use url::form_urlencoded;

use crate::refs::{EntityRef, Subject};

const PAIR_SEPARATOR: char = '&';
const VALUE_SEPARATOR: char = '=';
const QUERY_FORMS: [Form; 4] = [Form::Undashed, Form::Dashed, Form::Name, Form::Display];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryParam {
    pub name: String,
    pub raw: String,
    pub value: String,
    #[serde(rename = "ref")]
    pub entity: Option<EntityRef>,
}

pub fn query_params(query: Option<&str>, subjects: &[Subject]) -> Vec<QueryParam> {
    query
        .unwrap_or_default()
        .split(PAIR_SEPARATOR)
        .filter(|pair| !pair.is_empty())
        .map(|pair| query_param(pair, subjects))
        .collect()
}

fn query_param(pair: &str, subjects: &[Subject]) -> QueryParam {
    let raw = pair
        .split_once(VALUE_SEPARATOR)
        .map_or("", |(_, value)| value);
    let (name, value) = form_urlencoded::parse(pair.as_bytes())
        .next()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .unwrap_or_default();
    QueryParam {
        entity: subject_ref(&value, subjects),
        raw: raw.to_owned(),
        name,
        value,
    }
}

fn subject_ref(value: &str, subjects: &[Subject]) -> Option<EntityRef> {
    let value = value.trim();
    let subject = subjects
        .iter()
        .filter(|subject| QUERY_FORMS.contains(&subject.form))
        .find(|subject| subject.raw == value)?;
    match (subject.player, &subject.name) {
        (Some(uuid), _) => Some(EntityRef::Player { uuid }),
        (None, Some(name)) => Some(EntityRef::Name { name: name.clone() }),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    const OJUGG: &str = "94589c87999b4a7ab3add2ae50711e64";

    fn ojugg() -> Uuid {
        Uuid::parse_str(OJUGG).unwrap()
    }

    fn subject(player: Option<Uuid>, name: Option<&str>, raw: &str, form: Form) -> Subject {
        Subject {
            player,
            name: name.map(str::to_owned),
            raw: raw.to_owned(),
            form,
        }
    }

    #[test]
    fn every_pair_keeps_its_raw_text_and_its_decoded_value() {
        let params = query_params(Some("player=%20oJugg+&sources=GAME%2CPARTY&flag&=x"), &[]);
        let decoded: Vec<(&str, &str, &str)> = params
            .iter()
            .map(|param| {
                (
                    param.name.as_str(),
                    param.raw.as_str(),
                    param.value.as_str(),
                )
            })
            .collect();
        assert_eq!(
            decoded,
            vec![
                ("player", "%20oJugg+", " oJugg "),
                ("sources", "GAME%2CPARTY", "GAME,PARTY"),
                ("flag", "", ""),
                ("", "x", "x"),
            ]
        );
    }

    #[test]
    fn values_that_are_subjects_link_to_their_player_or_name() {
        let subjects = [
            subject(Some(ojugg()), None, OJUGG, Form::Undashed),
            subject(None, Some("somenick"), "SomeNick", Form::Display),
            subject(Some(ojugg()), Some("ojugg"), "oJugg", Form::Path),
        ];
        let params = query_params(
            Some(&format!("uuid={OJUGG}&name=SomeNick&key=oJugg")),
            &subjects,
        );
        let refs: Vec<Option<EntityRef>> = params.into_iter().map(|param| param.entity).collect();
        assert_eq!(
            refs,
            vec![
                Some(EntityRef::Player { uuid: ojugg() }),
                Some(EntityRef::Name {
                    name: "somenick".into()
                }),
                None,
            ]
        );
    }

    #[test]
    fn a_missing_or_empty_query_has_no_params() {
        assert!(query_params(None, &[]).is_empty());
        assert!(query_params(Some(""), &[]).is_empty());
        assert!(query_params(Some("&&"), &[]).is_empty());
    }
}
