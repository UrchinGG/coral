use reqlog::Form;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::forms::FORMER_NAME_CODE;
use crate::query::PlayerKeys;

type Sql = QueryBuilder<'static, Postgres>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNames {
    player: Uuid,
    current: Option<String>,
}

impl ResolvedNames {
    pub fn of(player: Uuid, keys: &PlayerKeys) -> Self {
        Self {
            player,
            current: keys
                .names
                .iter()
                .find(|held| held.to.is_none())
                .map(|held| held.name.clone()),
        }
    }

    pub fn player(&self) -> Uuid {
        self.player
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn push_category(&self, alias: &str, sql: &mut Sql) {
        let Some(current) = &self.current else {
            sql.push(format!("{alias}.form"));
            return;
        };
        sql.push(format!(
            "(CASE WHEN {alias}.form = {} AND {alias}.name IS DISTINCT FROM ",
            Form::Name.code()
        ))
        .push_bind(current.clone())
        .push(format!(
            "::text THEN {FORMER_NAME_CODE} ELSE {alias}.form END)::int2"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::HeldName;

    fn keys(names: &[(&str, bool)]) -> PlayerKeys {
        PlayerKeys {
            uuids: Vec::new(),
            names: names
                .iter()
                .map(|(name, current)| HeldName {
                    name: name.to_string(),
                    from: None,
                    to: (!current).then(chrono::Utc::now),
                })
                .collect(),
        }
    }

    #[test]
    fn the_current_name_is_the_one_still_held() {
        let resolved =
            ResolvedNames::of(Uuid::nil(), &keys(&[("kes_tr", false), ("kestrel_", true)]));
        assert_eq!(resolved.current.as_deref(), Some("kestrel_"));
    }

    #[test]
    fn without_a_current_name_no_lookup_counts_as_former() {
        let resolved = ResolvedNames::of(Uuid::nil(), &keys(&[("kes_tr", false)]));
        let mut sql = Sql::new("");
        resolved.push_category("s", &mut sql);
        assert_eq!(sql.sql(), "s.form");
    }
}
