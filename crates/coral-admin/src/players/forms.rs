use std::collections::BTreeSet;

use chrono::TimeDelta;
use reqlog::Form;
use serde::{Deserialize, Serialize};

use crate::error::AdminError;

pub const FORMER_NAME_CODE: i16 = 7;

const FORM_SEPARATOR: char = ',';

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerForm {
    Undashed,
    Dashed,
    Name,
    Former,
    Path,
    Body,
    Display,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
pub enum PlayerWindow {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "14d")]
    #[default]
    Fortnight,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormSelection(BTreeSet<PlayerForm>);

impl PlayerForm {
    pub const ALL: [Self; 7] = [
        Self::Undashed,
        Self::Dashed,
        Self::Name,
        Self::Former,
        Self::Path,
        Self::Body,
        Self::Display,
    ];

    pub fn code(self) -> i16 {
        match self {
            Self::Undashed => Form::Undashed.code(),
            Self::Dashed => Form::Dashed.code(),
            Self::Name => Form::Name.code(),
            Self::Former => FORMER_NAME_CODE,
            Self::Path => Form::Path.code(),
            Self::Body => Form::Body.code(),
            Self::Display => Form::Display.code(),
        }
    }

    pub fn stored_code(self) -> i16 {
        match self {
            Self::Former => Form::Name.code(),
            form => form.code(),
        }
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|form| form.code() == code)
    }

    pub fn from_stored(code: i16, former: bool) -> Option<Self> {
        match Self::from_code(code)? {
            Self::Name if former => Some(Self::Former),
            form => Some(form),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Undashed => "undashed",
            Self::Dashed => "dashed",
            Self::Name => "name",
            Self::Former => "former",
            Self::Path => "path",
            Self::Body => "body",
            Self::Display => "display",
        }
    }

    fn parse(raw: &str) -> Result<Self, AdminError> {
        Self::ALL
            .into_iter()
            .find(|form| form.label() == raw)
            .ok_or_else(|| AdminError::BadRequest(format!("{raw:?} is not a player lookup form")))
    }
}

impl PlayerWindow {
    pub fn length(self) -> TimeDelta {
        match self {
            Self::Day => TimeDelta::hours(24),
            Self::Week => TimeDelta::days(7),
            Self::Fortnight => TimeDelta::days(14),
        }
    }

    pub fn widest() -> Self {
        Self::Fortnight
    }

    pub fn covers(self, age: Self) -> bool {
        age.length() <= self.length()
    }
}

impl FormSelection {
    pub fn standard() -> Self {
        Self(
            PlayerForm::ALL
                .into_iter()
                .filter(|form| *form != PlayerForm::Display)
                .collect(),
        )
    }

    pub fn parse(forms: Option<&str>, include_display: bool) -> Result<Self, AdminError> {
        let mut selected = match forms.map(str::trim).filter(|raw| !raw.is_empty()) {
            Some(raw) => raw
                .split(FORM_SEPARATOR)
                .map(|form| PlayerForm::parse(form.trim()))
                .collect::<Result<BTreeSet<_>, _>>()?,
            None => Self::standard().0,
        };
        if include_display {
            selected.insert(PlayerForm::Display);
        }
        Ok(Self(selected))
    }

    pub fn contains(&self, form: PlayerForm) -> bool {
        self.0.contains(&form)
    }

    pub fn codes(&self) -> Vec<i16> {
        self.0.iter().map(|form| form.code()).collect()
    }

    pub fn stored_codes(&self) -> Vec<i16> {
        let mut codes: Vec<i16> = self.0.iter().map(|form| form.stored_code()).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    pub fn splits_names(&self) -> bool {
        self.contains(PlayerForm::Name) != self.contains(PlayerForm::Former)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_codes_round_trip_and_former_names_get_their_own_code() {
        for form in PlayerForm::ALL {
            assert_eq!(PlayerForm::from_code(form.code()), Some(form));
        }
        assert_eq!(PlayerForm::Name.code(), Form::Name.code());
        assert_eq!(PlayerForm::Former.code(), FORMER_NAME_CODE);
        assert_eq!(PlayerForm::from_code(0), None);
        assert_eq!(
            PlayerForm::from_stored(Form::Name.code(), true),
            Some(PlayerForm::Former)
        );
        assert_eq!(
            PlayerForm::from_stored(Form::Path.code(), true),
            Some(PlayerForm::Path)
        );
    }

    #[test]
    fn the_standard_selection_leaves_display_names_out() {
        let standard = FormSelection::standard();
        assert!(!standard.contains(PlayerForm::Display));
        assert_eq!(standard.codes().len(), PlayerForm::ALL.len() - 1);
        assert_eq!(FormSelection::parse(None, false).unwrap(), standard);
        assert!(
            FormSelection::parse(None, true)
                .unwrap()
                .contains(PlayerForm::Display)
        );
    }

    #[test]
    fn listed_forms_replace_the_standard_selection() {
        let selection = FormSelection::parse(Some("body, former"), false).unwrap();
        assert!(selection.contains(PlayerForm::Body));
        assert!(selection.contains(PlayerForm::Former));
        assert!(!selection.contains(PlayerForm::Undashed));
        assert_eq!(
            FormSelection::parse(Some("body,nope"), false)
                .unwrap_err()
                .code(),
            "bad_request"
        );
    }

    #[test]
    fn former_names_are_stored_as_names_and_split_only_when_one_side_is_chosen() {
        let both = FormSelection::standard();
        assert!(!both.splits_names());
        let stored = [
            Form::Undashed,
            Form::Dashed,
            Form::Name,
            Form::Path,
            Form::Body,
        ];
        assert_eq!(both.stored_codes(), stored.map(Form::code).to_vec());
        let former = FormSelection::parse(Some("former,path"), false).unwrap();
        assert!(former.splits_names());
        assert_eq!(
            former.stored_codes(),
            vec![Form::Name.code(), Form::Path.code()]
        );
        let neither = FormSelection::parse(Some("body"), false).unwrap();
        assert!(!neither.splits_names());
    }

    #[test]
    fn wider_windows_cover_narrower_ages() {
        assert!(PlayerWindow::Fortnight.covers(PlayerWindow::Day));
        assert!(PlayerWindow::Week.covers(PlayerWindow::Week));
        assert!(!PlayerWindow::Day.covers(PlayerWindow::Week));
        assert_eq!(PlayerWindow::default(), PlayerWindow::widest());
    }
}
