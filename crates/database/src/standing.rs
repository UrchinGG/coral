use crate::{AccessRank, Member, MemberRepository};

pub const VOTE_GRANT_APPROVALS: i64 = 5;
pub const TAG_GRANT_CORRECT: i64 = 15;
pub const KEEP_RATIO: i64 = 7;
pub const EARN_RATIO: i64 = 10;
pub const BONUS_WEIGHT: i64 = 5;
pub const REVOKE_STRIKES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub can_vote: bool,
    pub can_tag: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StandingChange {
    pub vote: Option<bool>,
    pub tag: Option<bool>,
}

/// A staff-pinned tier that replaces whatever the automatic rules would decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandingOverride {
    Restricted,
    Submitter,
    Reviewer,
    Trusted,
}

pub const OVERRIDE_TIERS: [StandingOverride; 4] = [
    StandingOverride::Restricted,
    StandingOverride::Submitter,
    StandingOverride::Reviewer,
    StandingOverride::Trusted,
];

impl StandingOverride {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "restricted" => Some(Self::Restricted),
            "submitter" => Some(Self::Submitter),
            "reviewer" => Some(Self::Reviewer),
            "trusted" => Some(Self::Trusted),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Restricted => "restricted",
            Self::Submitter => "submitter",
            Self::Reviewer => "reviewer",
            Self::Trusted => "trusted",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Restricted => "Restricted",
            Self::Submitter => "Submitter",
            Self::Reviewer => "Reviewer",
            Self::Trusted => "Trusted",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Restricted => "No tagging, no voting",
            Self::Submitter => "Tags go through review",
            Self::Reviewer => "Can vote on tag reviews",
            Self::Trusted => "Can tag directly, skipping review",
        }
    }

    pub fn standing(self) -> Standing {
        match self {
            Self::Restricted | Self::Submitter => Standing {
                can_vote: false,
                can_tag: false,
            },
            Self::Reviewer => Standing {
                can_vote: true,
                can_tag: false,
            },
            Self::Trusted => Standing {
                can_vote: true,
                can_tag: true,
            },
        }
    }

    pub fn level(self) -> i16 {
        match self {
            Self::Restricted => -1,
            Self::Submitter | Self::Reviewer => 0,
            Self::Trusted => 1,
        }
    }
}

pub fn override_of(member: &Member) -> Option<StandingOverride> {
    member
        .standing_override
        .as_deref()
        .and_then(StandingOverride::parse)
}

pub fn strike_count(member: &Member) -> usize {
    member.strikes.as_array().map_or(0, Vec::len)
}

pub fn effective_level(member: &Member) -> i16 {
    if member.access_level >= AccessRank::Helper.to_level() {
        return member.access_level;
    }
    if let Some(over) = override_of(member) {
        return over.level();
    }
    if strike_count(member) >= REVOKE_STRIKES {
        return -1;
    }
    if member.tag_granted { 1 } else { 0 }
}

/// Standing as the automatic rules see it, ignoring any staff override.
pub fn evaluate_auto(member: &Member) -> Standing {
    let strikes = strike_count(member);
    Standing {
        can_vote: eval_vote(member, strikes),
        can_tag: eval_tag(member, strikes),
    }
}

pub fn evaluate(member: &Member) -> Standing {
    match override_of(member) {
        Some(over) => over.standing(),
        None => evaluate_auto(member),
    }
}

pub fn is_trusted(member: &Member) -> bool {
    evaluate(member).can_tag
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StandingExplanation {
    pub can_vote: bool,
    pub vote_reason: String,
    pub can_tag: bool,
    pub tag_reason: String,
}

pub fn explain(member: &Member) -> StandingExplanation {
    let strikes = strike_count(member);
    let (auto_vote, vote_reason) = explain_vote(member, strikes);
    let (auto_tag, tag_reason) = explain_tag(member, strikes);

    let Some(over) = override_of(member) else {
        return StandingExplanation {
            can_vote: auto_vote,
            vote_reason,
            can_tag: auto_tag,
            tag_reason,
        };
    };

    let forced = over.standing();
    StandingExplanation {
        can_vote: forced.can_vote,
        vote_reason: override_reason(over, &vote_reason),
        can_tag: forced.can_tag,
        tag_reason: override_reason(over, &tag_reason),
    }
}

fn override_reason(over: StandingOverride, auto_reason: &str) -> String {
    format!(
        "set by staff — {} (automatic: {auto_reason})",
        over.label()
    )
}

fn explain_vote(member: &Member, strikes: usize) -> (bool, String) {
    if strikes >= REVOKE_STRIKES {
        return (
            false,
            format!("revoked — {strikes} strikes (≥{REVOKE_STRIKES} revokes standing)"),
        );
    }
    let (approved, rejected) = (member.accepted_tags, member.rejected_tags);
    if member.vote_granted {
        if rejected == 0 {
            (true, format!("kept — {approved} accepted, no rejections"))
        } else if approved >= KEEP_RATIO * rejected {
            (
                true,
                format!(
                    "kept — {approved} accepted / {rejected} rejected (≥{KEEP_RATIO}:1 keep threshold)"
                ),
            )
        } else {
            (
                false,
                format!(
                    "lost — {approved} accepted / {rejected} rejected (below {KEEP_RATIO}:1 keep threshold)"
                ),
            )
        }
    } else if rejected == 0 {
        if approved >= VOTE_GRANT_APPROVALS {
            (
                true,
                format!(
                    "granted — {approved} accepted, no rejections (≥{VOTE_GRANT_APPROVALS} required)"
                ),
            )
        } else {
            (
                false,
                format!(
                    "not yet — {approved}/{VOTE_GRANT_APPROVALS} accepted needed, no rejections"
                ),
            )
        }
    } else if approved >= EARN_RATIO * rejected {
        (
            true,
            format!(
                "granted — {approved} accepted / {rejected} rejected (≥{EARN_RATIO}:1 earn threshold)"
            ),
        )
    } else {
        (
            false,
            format!(
                "not yet — {approved} accepted / {rejected} rejected (below {EARN_RATIO}:1 earn threshold)"
            ),
        )
    }
}

fn explain_tag(member: &Member, strikes: usize) -> (bool, String) {
    if strikes >= REVOKE_STRIKES {
        return (
            false,
            format!("revoked — {strikes} strikes (≥{REVOKE_STRIKES} revokes standing)"),
        );
    }
    let correct = member.accurate_verdicts + BONUS_WEIGHT * member.bonus_verdicts;
    let incorrect = member.incorrect_verdicts;
    if member.tag_granted {
        if incorrect == 0 {
            (
                true,
                format!("kept — {correct} correct, no incorrect verdicts"),
            )
        } else if correct >= KEEP_RATIO * incorrect {
            (
                true,
                format!(
                    "kept — {correct} correct / {incorrect} incorrect (≥{KEEP_RATIO}:1 keep threshold)"
                ),
            )
        } else {
            (
                false,
                format!(
                    "lost — {correct} correct / {incorrect} incorrect (below {KEEP_RATIO}:1 keep threshold)"
                ),
            )
        }
    } else if strikes != 0 {
        (
            false,
            format!("not yet — {strikes} strike(s) block a fresh grant"),
        )
    } else if correct < TAG_GRANT_CORRECT {
        (
            false,
            format!("not yet — {correct}/{TAG_GRANT_CORRECT} correct verdicts needed"),
        )
    } else if incorrect == 0 || correct >= EARN_RATIO * incorrect {
        (
            true,
            format!(
                "granted — {correct} correct / {incorrect} incorrect (≥{TAG_GRANT_CORRECT} correct, ≥{EARN_RATIO}:1 earn threshold)"
            ),
        )
    } else {
        (
            false,
            format!(
                "not yet — {correct} correct / {incorrect} incorrect (below {EARN_RATIO}:1 earn threshold)"
            ),
        )
    }
}

/// The flags a refresh would persist, and the effective change it should
/// report. Split out from [`refresh`] so the invariant is testable without a
/// database.
///
/// The granted flags stay the pure automatic state so that clearing an override
/// drops the member back exactly where the rules had them. While an override is
/// active the effective standing can't move, so nothing is reported and the
/// trusted role is left alone.
fn refresh_plan(member: &Member) -> (Standing, StandingChange) {
    let auto = evaluate_auto(member);
    if override_of(member).is_some() {
        return (auto, StandingChange::default());
    }
    let change = StandingChange {
        vote: (auto.can_vote != member.vote_granted).then_some(auto.can_vote),
        tag: (auto.can_tag != member.tag_granted).then_some(auto.can_tag),
    };
    (auto, change)
}

pub async fn refresh(
    repo: &MemberRepository<'_>,
    discord_id: i64,
) -> Result<StandingChange, sqlx::Error> {
    let Some(member) = repo.get_by_discord_id(discord_id).await? else {
        return Ok(StandingChange::default());
    };
    let (auto, change) = refresh_plan(&member);
    if auto.can_vote != member.vote_granted {
        repo.set_vote_granted(discord_id, auto.can_vote).await?;
    }
    if auto.can_tag != member.tag_granted {
        repo.set_tag_granted(discord_id, auto.can_tag).await?;
    }
    Ok(change)
}

fn eval_vote(member: &Member, strikes: usize) -> bool {
    if strikes >= REVOKE_STRIKES {
        return false;
    }
    let (approved, rejected) = (member.accepted_tags, member.rejected_tags);
    if member.vote_granted {
        rejected == 0 || approved >= KEEP_RATIO * rejected
    } else if rejected == 0 {
        approved >= VOTE_GRANT_APPROVALS
    } else {
        approved >= EARN_RATIO * rejected
    }
}

fn eval_tag(member: &Member, strikes: usize) -> bool {
    if strikes >= REVOKE_STRIKES {
        return false;
    }
    let correct = member.accurate_verdicts + BONUS_WEIGHT * member.bonus_verdicts;
    let incorrect = member.incorrect_verdicts;
    if member.tag_granted {
        incorrect == 0 || correct >= KEEP_RATIO * incorrect
    } else {
        strikes == 0
            && correct >= TAG_GRANT_CORRECT
            && (incorrect == 0 || correct >= EARN_RATIO * incorrect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn member(level: i16, strikes: usize) -> Member {
        Member {
            id: 1,
            discord_id: 1,
            uuid: None,
            api_key: None,
            join_date: Utc::now(),
            request_count: 0,
            access_level: level,
            key_locked: false,
            tagging_disabled: false,
            accepted_tags: 0,
            rejected_tags: 0,
            accurate_verdicts: 0,
            incorrect_verdicts: 0,
            bonus_verdicts: 0,
            vote_granted: false,
            tag_granted: false,
            standing_override: None,
            standing_override_by: None,
            standing_override_at: None,
            config: json!({}),
            strikes: json!(vec![json!({}); strikes]),
        }
    }

    fn overridden(mut m: Member, over: StandingOverride) -> Member {
        m.standing_override = Some(over.as_str().into());
        m.standing_override_by = Some(99);
        m.standing_override_at = Some(Utc::now());
        m
    }

    #[test]
    fn vote_grant_needs_five_clean() {
        let mut m = member(0, 0);
        m.accepted_tags = 4;
        assert!(!evaluate(&m).can_vote);
        m.accepted_tags = 5;
        assert!(evaluate(&m).can_vote);
        m.rejected_tags = 1;
        assert!(!evaluate(&m).can_vote);
    }

    #[test]
    fn vote_reearn_at_ten_to_one() {
        let mut m = member(0, 0);
        m.accepted_tags = 9;
        m.rejected_tags = 1;
        assert!(!evaluate(&m).can_vote);
        m.accepted_tags = 10;
        assert!(evaluate(&m).can_vote);
    }

    #[test]
    fn vote_kept_until_below_seven_to_one() {
        let mut m = member(0, 0);
        m.vote_granted = true;
        m.accepted_tags = 7;
        m.rejected_tags = 1;
        assert!(evaluate(&m).can_vote);
        m.accepted_tags = 6;
        assert!(!evaluate(&m).can_vote);
    }

    #[test]
    fn tag_grant_needs_volume_ratio_and_no_strikes() {
        let mut m = member(0, 0);
        m.accurate_verdicts = 15;
        assert!(evaluate(&m).can_tag);
        m.accurate_verdicts = 14;
        assert!(!evaluate(&m).can_tag);
        m.bonus_verdicts = 1;
        assert!(evaluate(&m).can_tag);
        m.incorrect_verdicts = 1;
        assert!(evaluate(&m).can_tag);
        m.incorrect_verdicts = 2;
        assert!(!evaluate(&m).can_tag);
    }

    #[test]
    fn one_strike_blocks_tag_grant_but_not_keep() {
        let mut m = member(0, 1);
        m.accurate_verdicts = 20;
        assert!(!evaluate(&m).can_tag);
        m.tag_granted = true;
        assert!(evaluate(&m).can_tag);
    }

    #[test]
    fn two_strikes_revoke_everything() {
        let mut m = member(0, 2);
        m.accepted_tags = 50;
        m.accurate_verdicts = 50;
        m.vote_granted = true;
        m.tag_granted = true;
        let s = evaluate(&m);
        assert!(!s.can_vote);
        assert!(!s.can_tag);
        assert_eq!(effective_level(&m), -1);
    }

    #[test]
    fn effective_level_reflects_standing_not_rank() {
        let mut m = member(0, 0);
        assert_eq!(effective_level(&m), 0);
        m.tag_granted = true;
        assert_eq!(effective_level(&m), 1);
        m.access_level = 3;
        assert_eq!(effective_level(&m), 3);
    }

    #[test]
    fn explain_matches_evaluate_across_scenarios() {
        let scenarios: Vec<Box<dyn Fn(&mut Member)>> = vec![
            Box::new(|m| {
                m.accepted_tags = 4;
            }),
            Box::new(|m| {
                m.accepted_tags = 5;
            }),
            Box::new(|m| {
                m.vote_granted = true;
                m.accepted_tags = 7;
                m.rejected_tags = 1;
            }),
            Box::new(|m| {
                m.vote_granted = true;
                m.accepted_tags = 6;
                m.rejected_tags = 1;
            }),
            Box::new(|m| {
                m.accurate_verdicts = 15;
            }),
            Box::new(|m| {
                m.accurate_verdicts = 14;
            }),
            Box::new(|m| {
                m.tag_granted = true;
                m.accurate_verdicts = 20;
                m.incorrect_verdicts = 3;
            }),
        ];
        let overrides = [None].into_iter().chain(OVERRIDE_TIERS.map(Some));
        for setup in scenarios {
            for over in overrides.clone() {
                let mut m = member(0, 0);
                setup(&mut m);
                if let Some(over) = over {
                    m = overridden(m, over);
                }
                let evaluated = evaluate(&m);
                let explanation = explain(&m);
                assert_eq!(evaluated.can_vote, explanation.can_vote);
                assert_eq!(evaluated.can_tag, explanation.can_tag);
                assert!(!explanation.vote_reason.is_empty());
                assert!(!explanation.tag_reason.is_empty());
            }
        }
    }

    fn apply_refresh(m: &mut Member) -> StandingChange {
        let (auto, change) = refresh_plan(m);
        m.vote_granted = auto.can_vote;
        m.tag_granted = auto.can_tag;
        change
    }

    #[test]
    fn override_forces_standing_regardless_of_record() {
        let blank = overridden(member(0, 0), StandingOverride::Trusted);
        let s = evaluate(&blank);
        assert!(s.can_tag);
        assert!(s.can_vote);
        assert!(!evaluate_auto(&blank).can_tag);

        let mut veteran = member(0, 0);
        veteran.accepted_tags = 100;
        veteran.accurate_verdicts = 100;
        veteran.vote_granted = true;
        veteran.tag_granted = true;
        let veteran = overridden(veteran, StandingOverride::Restricted);
        let s = evaluate(&veteran);
        assert!(!s.can_tag);
        assert!(!s.can_vote);
        assert!(evaluate_auto(&veteran).can_tag);
    }

    #[test]
    fn override_beats_strikes_not_staff_rank() {
        let struck = overridden(member(0, 2), StandingOverride::Trusted);
        assert!(evaluate(&struck).can_tag);
        assert_eq!(effective_level(&struck), 1);

        let staff = overridden(member(3, 0), StandingOverride::Restricted);
        assert_eq!(effective_level(&staff), 3);
    }

    #[test]
    fn effective_level_maps_each_tier() {
        let expected = [(-1), 0, 0, 1];
        for (tier, want) in OVERRIDE_TIERS.into_iter().zip(expected) {
            let m = overridden(member(0, 0), tier);
            assert_eq!(effective_level(&m), want, "tier {}", tier.as_str());
        }
    }

    #[test]
    fn refresh_leaves_hysteresis_intact_under_override() {
        let mut natural = member(0, 0);
        natural.accurate_verdicts = 20;
        natural.accepted_tags = 10;
        let mut forced = overridden(natural.clone(), StandingOverride::Restricted);

        for _ in 0..3 {
            apply_refresh(&mut natural);
            apply_refresh(&mut forced);
        }

        forced.standing_override = None;
        forced.standing_override_by = None;
        forced.standing_override_at = None;

        assert_eq!(forced.tag_granted, natural.tag_granted);
        assert_eq!(forced.vote_granted, natural.vote_granted);
        assert_eq!(evaluate(&forced), evaluate(&natural));
    }

    #[test]
    fn refresh_reports_no_change_while_overridden() {
        let mut m = member(0, 0);
        m.accurate_verdicts = 20;
        m.accepted_tags = 10;
        let mut forced = overridden(m.clone(), StandingOverride::Trusted);

        assert_eq!(apply_refresh(&mut m), StandingChange {
            vote: Some(true),
            tag: Some(true)
        });
        assert_eq!(apply_refresh(&mut forced), StandingChange::default());
        assert!(forced.tag_granted, "auto flags still tracked underneath");
    }

    #[test]
    fn explain_mentions_override_and_keeps_automatic_reason() {
        let m = overridden(member(0, 0), StandingOverride::Trusted);
        let explanation = explain(&m);
        assert!(explanation.can_tag);
        assert!(explanation.tag_reason.contains("set by staff — Trusted"));
        assert!(explanation.tag_reason.contains("0/15 correct verdicts"));
    }

    #[test]
    fn explain_revoked_mentions_strike_count() {
        let m = member(0, 2);
        let explanation = explain(&m);
        assert!(!explanation.can_vote);
        assert!(!explanation.can_tag);
        assert!(explanation.vote_reason.contains("2 strikes"));
        assert!(explanation.tag_reason.contains("2 strikes"));
    }

    #[test]
    fn explain_vote_kept_reason_shows_ratio() {
        let mut m = member(0, 0);
        m.vote_granted = true;
        m.accepted_tags = 14;
        m.rejected_tags = 2;
        let explanation = explain(&m);
        assert!(explanation.can_vote);
        assert!(explanation.vote_reason.contains("14 accepted"));
        assert!(explanation.vote_reason.contains("2 rejected"));
    }

    #[test]
    fn explain_tag_not_yet_blocked_by_strike() {
        let mut m = member(0, 1);
        m.accurate_verdicts = 20;
        let explanation = explain(&m);
        assert!(!explanation.can_tag);
        assert!(explanation.tag_reason.contains("strike"));
    }
}
