//! Pure: given a server's settings, a member's context and what they have now,
//! decides the nickname and roles they should have. No Discord, database or network.

use serde_json::Value;
use serenity::all::RoleId;

use database::{GuildConfig, GuildRoleRule};

pub const NICKNAME_MAX_LEN: usize = 32;
const NICKNAME_FIELD: &str = "discord.nickname";

pub struct Settings {
    pub nickname_template: Option<String>,
    pub link_role: Option<RoleId>,
    pub unlinked_role: Option<RoleId>,
    pub rules: Vec<Rule>,
}

pub struct Rule {
    pub role: RoleId,
    pub condition: String,
}

pub struct Current<'a> {
    pub roles: &'a [RoleId],
    pub nick: Option<&'a str>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    pub roles: Option<Vec<RoleId>>,
    pub nickname: Option<String>,
}

impl Settings {
    pub fn new(config: &GuildConfig, rules: &[GuildRoleRule]) -> Self {
        Self {
            nickname_template: config.nickname_template.clone(),
            link_role: config.link_role_id.map(|id| RoleId::new(id as u64)),
            unlinked_role: config.unlinked_role_id.map(|id| RoleId::new(id as u64)),
            rules: rules
                .iter()
                .map(|rule| Rule {
                    role: RoleId::new(rule.role_id as u64),
                    condition: rule.condition.clone(),
                })
                .collect(),
        }
    }

    pub fn syncs_anything(&self) -> bool {
        self.nickname_template.is_some() || !self.rules.is_empty()
    }
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.roles.is_none() && self.nickname.is_none()
    }
}

/// `nickname_frozen` covers a paused member or one holding the bypass role.
/// `preserve_custom` keeps what a member typed after the template's own text,
/// for templates that place `discord.nickname`.
pub fn linked_changes(
    settings: &Settings,
    context: &Value,
    current: &Current,
    nickname_frozen: bool,
    preserve_custom: bool,
) -> Changes {
    let mut roles = current.roles.to_vec();
    if let Some(role) = settings.link_role {
        add(&mut roles, role);
    }
    if let Some(role) = settings.unlinked_role {
        remove(&mut roles, role);
    }
    for rule in &settings.rules {
        if expr::eval_condition(&rule.condition, context).unwrap_or(false) {
            add(&mut roles, rule.role);
        } else {
            remove(&mut roles, rule.role);
        }
    }

    let nickname = match &settings.nickname_template {
        Some(template) if !nickname_frozen => {
            render_nickname(template, context, current.nick, preserve_custom)
        }
        _ => None,
    };
    Changes {
        roles: (roles != current.roles).then_some(roles),
        nickname,
    }
}

pub fn unlinked_changes(settings: &Settings, current: &Current) -> Changes {
    let mut roles = current.roles.to_vec();
    if let Some(role) = settings.unlinked_role {
        add(&mut roles, role);
    }
    if let Some(role) = settings.link_role {
        remove(&mut roles, role);
    }
    for rule in &settings.rules {
        remove(&mut roles, rule.role);
    }
    Changes {
        roles: (roles != current.roles).then_some(roles),
        nickname: None,
    }
}

fn render_nickname(
    template: &str,
    context: &Value,
    current: Option<&str>,
    preserve_custom: bool,
) -> Option<String> {
    let nick = if preserve_custom && template.contains(NICKNAME_FIELD) {
        let mut context = context.clone();
        let custom = custom_suffix(template, &mut context, current);
        context["discord"]["nickname"] = Value::String(custom);
        expr::render_template(template, &context).to_truncated(NICKNAME_MAX_LEN)
    } else {
        expr::render_template(template, context).to_truncated(NICKNAME_MAX_LEN)
    };
    (!nick.is_empty() && current != Some(nick.as_str())).then_some(nick)
}

fn custom_suffix(template: &str, context: &mut Value, current: Option<&str>) -> String {
    let current = match current {
        Some(nick) if !nick.is_empty() => nick,
        _ => return String::new(),
    };
    context["discord"]["nickname"] = Value::String(String::new());
    let base = expr::render_template(template, context).to_truncated(NICKNAME_MAX_LEN);
    current
        .strip_prefix(base.trim_end())
        .map(|rest| rest.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

fn add(roles: &mut Vec<RoleId>, role: RoleId) {
    if !roles.contains(&role) {
        roles.push(role);
    }
}

fn remove(roles: &mut Vec<RoleId>, role: RoleId) {
    roles.retain(|r| *r != role);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LINKED: RoleId = RoleId::new(1);
    const UNLINKED: RoleId = RoleId::new(2);
    const PRESTIGE: RoleId = RoleId::new(3);
    const OTHER: RoleId = RoleId::new(9);

    fn settings(template: Option<&str>) -> Settings {
        Settings {
            nickname_template: template.map(String::from),
            link_role: Some(LINKED),
            unlinked_role: Some(UNLINKED),
            rules: vec![Rule {
                role: PRESTIGE,
                condition: "achievements.bedwars_level >= 100".into(),
            }],
        }
    }

    fn context(level: i64) -> Value {
        json!({"displayname": "Nelly", "achievements": {"bedwars_level": level}, "discord": {"name": "nelly"}})
    }

    fn current<'a>(roles: &'a [RoleId], nick: Option<&'a str>) -> Current<'a> {
        Current { roles, nick }
    }

    #[test]
    fn a_linked_member_gains_the_link_role_and_matching_rules() {
        let changes = linked_changes(
            &settings(None),
            &context(150),
            &current(&[OTHER, UNLINKED], None),
            false,
            false,
        );
        assert_eq!(changes.roles, Some(vec![OTHER, LINKED, PRESTIGE]));
        assert_eq!(changes.nickname, None);
    }

    #[test]
    fn a_rule_that_stops_matching_takes_its_role_away() {
        let changes = linked_changes(
            &settings(None),
            &context(50),
            &current(&[LINKED, PRESTIGE], None),
            false,
            false,
        );
        assert_eq!(changes.roles, Some(vec![LINKED]));
    }

    #[test]
    fn nothing_changes_when_the_member_is_already_right() {
        let changes = linked_changes(
            &settings(Some("{displayname}")),
            &context(150),
            &current(&[LINKED, PRESTIGE], Some("Nelly")),
            false,
            false,
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn the_template_renders_unless_the_nickname_is_frozen() {
        let template = settings(Some("{displayname}"));
        let roles = [LINKED, PRESTIGE];
        let fresh = linked_changes(
            &template,
            &context(150),
            &current(&roles, None),
            false,
            false,
        );
        assert_eq!(fresh.nickname.as_deref(), Some("Nelly"));
        let frozen = linked_changes(
            &template,
            &context(150),
            &current(&roles, None),
            true,
            false,
        );
        assert_eq!(frozen.nickname, None);
    }

    #[test]
    fn a_custom_suffix_survives_when_preserved() {
        let template = settings(Some("{displayname} {discord.nickname}"));
        let roles = [LINKED, PRESTIGE];
        let kept = linked_changes(
            &template,
            &context(150),
            &current(&roles, Some("Nelly gg")),
            false,
            true,
        );
        assert_eq!(kept.nickname, None, "already right");
        let dropped = linked_changes(
            &template,
            &context(150),
            &current(&roles, Some("Nelly gg")),
            false,
            false,
        );
        assert_eq!(
            dropped.nickname.as_deref(),
            Some("Nelly "),
            "an empty custom part leaves the separator"
        );
    }

    #[test]
    fn an_unlinked_member_loses_link_and_rule_roles() {
        let changes = unlinked_changes(&settings(None), &current(&[LINKED, PRESTIGE, OTHER], None));
        assert_eq!(changes.roles, Some(vec![OTHER, UNLINKED]));
        assert_eq!(changes.nickname, None);
    }

    #[test]
    fn a_missing_field_matches_no_comparison_and_renders_empty() {
        let context = json!({"displayname": "Nelly"});
        let eval = |condition| expr::eval_condition(condition, &context).unwrap();
        assert!(!eval("rbw.elo >= 1000"));
        assert!(
            !eval("rbw.elo < 1000"),
            "a member the source does not know has no elo"
        );
        assert!(!eval("rbw.ranked"));
        assert_eq!(
            expr::render_template("[{rbw.elo}] {displayname}", &context).to_truncated(32),
            "[] Nelly"
        );
    }
}
