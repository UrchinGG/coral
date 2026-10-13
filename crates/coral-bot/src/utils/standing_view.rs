use serenity::all::*;

use database::{
    Member, Strike,
    standing::{
        EARN_RATIO, REVOKE_STRIKES, TAG_GRANT_CORRECT, VOTE_GRANT_APPROVALS, correct_votes,
        evaluate, strike_count, tag_target, vote_target,
    },
};

use crate::framework::AccessRank;
use crate::utils::{sanitize_reason, text};

const TICKET_URL: &str =
    "https://discord.com/channels/1339318572069158962/1342235914746986556/1409941039824769034";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Submitter,
    Reviewer,
    Trusted,
    Suspended,
    Staff,
}

impl Tier {
    pub fn of(member: &Member, rank: AccessRank) -> Self {
        if rank >= AccessRank::Helper {
            return Tier::Staff;
        }
        if strike_count(member) >= REVOKE_STRIKES {
            return Tier::Suspended;
        }
        let standing = evaluate(member);
        if standing.can_tag {
            Tier::Trusted
        } else if standing.can_vote {
            Tier::Reviewer
        } else {
            Tier::Submitter
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tier::Submitter => "Submitter",
            Tier::Reviewer => "Reviewer",
            Tier::Trusted => "Trusted",
            Tier::Suspended => "Suspended",
            Tier::Staff => "Staff",
        }
    }
}

pub fn standing_section(member: &Member, rank: AccessRank) -> CreateContainerComponent<'static> {
    let tier = Tier::of(member, rank);
    let mut lines = vec![format!("### Standing: {}", tier.label())];
    lines.extend(capability_lines(member, tier));
    if member.tagging_disabled {
        lines.push("-# `/tag add` is disabled for your account.".to_string());
    }
    lines.extend(next_lines(member, tier));
    text(lines.join("\n"))
}

pub fn record_section(member: &Member, tags_added: i64) -> CreateContainerComponent<'static> {
    let resolved = match member.bonus_verdicts {
        0 => String::new(),
        1 => " (1 on a review staff resolved)".to_string(),
        n => format!(" ({n} on reviews staff resolved)"),
    };
    text(format!(
        "-# Tags added: {tags_added}\n\
         -# Submissions: {} accepted, {} rejected\n\
         -# Votes: {} correct{resolved}, {} incorrect",
        member.accepted_tags,
        member.rejected_tags,
        member.accurate_verdicts,
        member.incorrect_verdicts,
    ))
}

pub fn active_strikes_section(strikes: &[Strike]) -> Option<CreateContainerComponent<'static>> {
    let active: Vec<&Strike> = strikes.iter().filter(|s| s.is_active()).collect();
    if active.is_empty() {
        return None;
    }
    let mut lines = vec!["### Active Strikes".to_string()];
    for strike in active {
        lines.push(format!(
            "> {}\n> -# **\\-** {}",
            sanitize_reason(&strike.reason),
            expiry_phrase(strike)
        ));
    }
    lines.push(format!(
        "-# While a strike is active on your account, you cannot earn tagging or voting \
         permissions. After two active strikes, any existing permissions you have are \
         suspended. If you believe a strike should be removed, please \
         [open a ticket]({TICKET_URL})."
    ));
    Some(text(lines.join("\n")))
}

pub fn expiry_phrase(strike: &Strike) -> String {
    match strike.expires_at {
        Some(at) => format!("Expires <t:{}:R>", at.timestamp()),
        None => "No expiration".to_string(),
    }
}

fn capability_lines(member: &Member, tier: Tier) -> Vec<String> {
    let standing = evaluate(member);
    let staff = tier == Tier::Staff;
    let mut lines = Vec::new();
    if !matches!(tier, Tier::Trusted | Tier::Staff) {
        lines.push(capability(
            "Report players for review",
            !member.tagging_disabled,
        ));
    }
    lines.push(capability(
        "Vote on tag reviews",
        staff || standing.can_vote,
    ));
    lines.push(capability(
        "Tag players directly",
        (staff || standing.can_tag) && !member.tagging_disabled,
    ));
    lines
}

fn capability(label: &str, allowed: bool) -> String {
    if allowed {
        format!("{label}: **Yes**")
    } else {
        format!("-# {label}: No")
    }
}

fn next_lines(member: &Member, tier: Tier) -> Vec<String> {
    match tier {
        Tier::Submitter => progress_lines(
            "Reviewer",
            member.accepted_tags,
            vote_target(member),
            VOTE_GRANT_APPROVALS,
            "accepted submissions",
            "accepted to rejected submissions",
        ),
        Tier::Reviewer => progress_lines(
            "Trusted",
            correct_votes(member),
            tag_target(member),
            TAG_GRANT_CORRECT,
            "correct votes",
            "correct to incorrect votes",
        ),
        Tier::Trusted | Tier::Suspended | Tier::Staff => Vec::new(),
    }
}

fn progress_lines(
    next: &str,
    count: i64,
    target: i64,
    base: i64,
    unit: &str,
    ratio_of: &str,
) -> Vec<String> {
    let mut lines = vec![
        format!("**Next: {next}**"),
        format!("**{count}** of **{target}** {unit}"),
    ];
    if target > base {
        lines.push(format!(
            "-# You must maintain a {EARN_RATIO}:1 ratio of {ratio_of} to earn {next}."
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn member() -> Member {
        Member {
            id: 1,
            discord_id: 1,
            uuid: None,
            api_key: None,
            join_date: Utc::now(),
            request_count: 0,
            access_level: 0,
            key_locked: false,
            tagging_disabled: false,
            accepted_tags: 3,
            rejected_tags: 0,
            accurate_verdicts: 0,
            incorrect_verdicts: 0,
            bonus_verdicts: 0,
            vote_granted: false,
            tag_granted: false,
            config: json!({}),
            strikes: json!([]),
            active_strikes: 0,
        }
    }

    #[test]
    fn submitter_progress_has_no_ratio_line_until_a_rejection() {
        let m = member();
        let lines = next_lines(&m, Tier::Submitter);
        assert_eq!(lines[1], "**3** of **5** accepted submissions");
        assert_eq!(lines.len(), 2);

        let mut m = member();
        m.rejected_tags = 1;
        let lines = next_lines(&m, Tier::Submitter);
        assert_eq!(lines[1], "**3** of **12** accepted submissions");
        assert!(lines[2].contains("12:1 ratio of accepted to rejected submissions"));
    }

    #[test]
    fn trusted_drops_the_report_line() {
        let mut m = member();
        m.tag_granted = true;
        m.vote_granted = true;
        let tier = Tier::of(&m, AccessRank::Default);
        assert_eq!(tier, Tier::Trusted);
        let lines = capability_lines(&m, tier);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.ends_with("**Yes**")));
    }

    #[test]
    fn two_strikes_read_as_suspended() {
        let mut m = member();
        m.active_strikes = 2;
        m.tag_granted = true;
        assert_eq!(Tier::of(&m, AccessRank::Default), Tier::Suspended);
    }

    #[test]
    fn disabled_tagging_turns_both_tag_lines_off() {
        let mut m = member();
        m.tagging_disabled = true;
        let lines = capability_lines(&m, Tier::Submitter);
        assert_eq!(lines[0], "-# Report players for review: No");
        assert_eq!(lines[2], "-# Tag players directly: No");
    }
}
