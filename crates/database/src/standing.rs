use crate::{AccessRank, Member, MemberRepository, ReviewRules};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grants {
    Held,
    Ignored,
}

pub fn strike_count(member: &Member) -> usize {
    member.active_strikes.max(0) as usize
}

pub fn effective_level(member: &Member, rules: &ReviewRules) -> i16 {
    if member.access_level >= AccessRank::Helper.to_level() {
        return member.access_level;
    }
    if rules.suspends(strike_count(member)) {
        return -1;
    }
    if member.tag_granted { 1 } else { 0 }
}

pub fn evaluate(member: &Member, rules: &ReviewRules) -> Standing {
    evaluate_with(member, rules, Grants::Held)
}

pub fn evaluate_fresh(member: &Member, rules: &ReviewRules) -> Standing {
    evaluate_with(member, rules, Grants::Ignored)
}

fn evaluate_with(member: &Member, rules: &ReviewRules, grants: Grants) -> Standing {
    let strikes = strike_count(member);
    let held = grants == Grants::Held;
    let can_tag = eval_tag(member, rules, strikes, held && member.tag_granted);
    Standing {
        can_vote: can_tag || eval_vote(member, rules, strikes, held && member.vote_granted),
        can_tag,
    }
}

pub fn correct_votes(member: &Member, rules: &ReviewRules) -> i64 {
    member.accurate_verdicts + rules.bonus_weight * member.bonus_verdicts
}

pub fn vote_target(member: &Member, rules: &ReviewRules) -> i64 {
    rules
        .vote_grant_approvals
        .max(rules.earn_ratio * member.rejected_tags)
}

pub fn tag_target(member: &Member, rules: &ReviewRules) -> i64 {
    rules
        .tag_grant_correct
        .max(rules.earn_ratio * member.incorrect_verdicts)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StandingExplanation {
    pub can_vote: bool,
    pub vote_reason: String,
    pub can_tag: bool,
    pub tag_reason: String,
}

pub fn explain(member: &Member, rules: &ReviewRules) -> StandingExplanation {
    let strikes = strike_count(member);
    let (can_tag, tag_reason) = explain_tag(member, rules, strikes);
    let (can_vote, vote_reason) = match explain_vote(member, rules, strikes) {
        (false, _) if can_tag => (true, "eligible to tag".to_string()),
        vote => vote,
    };
    StandingExplanation {
        can_vote,
        vote_reason,
        can_tag,
        tag_reason,
    }
}

fn explain_vote(member: &Member, rules: &ReviewRules, strikes: usize) -> (bool, String) {
    let (approved, rejected) = (member.accepted_tags, member.rejected_tags);
    let target = vote_target(member, rules);
    explain_grant(
        rules,
        strikes,
        member.vote_granted,
        approved >= target,
        format!("{approved}/{target} accepted submissions ({rejected} rejected)"),
    )
}

fn explain_tag(member: &Member, rules: &ReviewRules, strikes: usize) -> (bool, String) {
    let correct = correct_votes(member, rules);
    let target = tag_target(member, rules);
    explain_grant(
        rules,
        strikes,
        member.tag_granted,
        correct >= target,
        format!(
            "{correct}/{target} correct votes ({} incorrect)",
            member.incorrect_verdicts
        ),
    )
}

fn explain_grant(
    rules: &ReviewRules,
    strikes: usize,
    held: bool,
    earned: bool,
    progress: String,
) -> (bool, String) {
    if rules.suspends(strikes) {
        return (
            false,
            format!(
                "suspended: {strikes} active strikes (\u{2265}{} suspend standing)",
                rules.revoke_strikes
            ),
        );
    }
    if held {
        return (true, format!("held: {progress}"));
    }
    if strikes > 0 {
        return (
            false,
            format!("not yet: an active strike blocks a fresh grant, {progress}"),
        );
    }
    if earned {
        (true, format!("earned: {progress}"))
    } else {
        (false, format!("not yet: {progress}"))
    }
}

pub async fn refresh(
    repo: &MemberRepository<'_>,
    rules: &ReviewRules,
    discord_id: i64,
) -> Result<StandingChange, sqlx::Error> {
    refresh_with(repo, rules, discord_id, Grants::Held).await
}

pub async fn refresh_fresh(
    repo: &MemberRepository<'_>,
    rules: &ReviewRules,
    discord_id: i64,
) -> Result<StandingChange, sqlx::Error> {
    refresh_with(repo, rules, discord_id, Grants::Ignored).await
}

async fn refresh_with(
    repo: &MemberRepository<'_>,
    rules: &ReviewRules,
    discord_id: i64,
    grants: Grants,
) -> Result<StandingChange, sqlx::Error> {
    let Some(member) = repo.get_by_discord_id(discord_id).await? else {
        return Ok(StandingChange::default());
    };
    let standing = evaluate_with(&member, rules, grants);
    let mut change = StandingChange::default();
    if standing.can_vote != member.vote_granted {
        repo.set_vote_granted(discord_id, standing.can_vote).await?;
        change.vote = Some(standing.can_vote);
    }
    if standing.can_tag != member.tag_granted {
        repo.set_tag_granted(discord_id, standing.can_tag).await?;
        change.tag = Some(standing.can_tag);
    }
    Ok(change)
}

fn eval_vote(member: &Member, rules: &ReviewRules, strikes: usize, held: bool) -> bool {
    if rules.suspends(strikes) {
        return false;
    }
    held || (strikes == 0 && member.accepted_tags >= vote_target(member, rules))
}

fn eval_tag(member: &Member, rules: &ReviewRules, strikes: usize, held: bool) -> bool {
    if rules.suspends(strikes) {
        return false;
    }
    held || (strikes == 0 && correct_votes(member, rules) >= tag_target(member, rules))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    const R: ReviewRules = ReviewRules::DEFAULT;

    fn member(level: i16, strikes: i64) -> Member {
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
            config: json!({}),
            strikes: json!([]),
            active_strikes: strikes,
        }
    }

    #[test]
    fn vote_grant_needs_five_clean() {
        let mut m = member(0, 0);
        m.accepted_tags = 4;
        assert!(!evaluate(&m, &R).can_vote);
        m.accepted_tags = 5;
        assert!(evaluate(&m, &R).can_vote);
        m.rejected_tags = 1;
        assert!(!evaluate(&m, &R).can_vote);
    }

    #[test]
    fn vote_earn_at_twelve_to_one() {
        let mut m = member(0, 0);
        m.accepted_tags = 11;
        m.rejected_tags = 1;
        assert!(!evaluate(&m, &R).can_vote);
        m.accepted_tags = 12;
        assert!(evaluate(&m, &R).can_vote);
    }

    #[test]
    fn held_grants_survive_any_ratio() {
        let mut m = member(0, 0);
        m.vote_granted = true;
        m.tag_granted = true;
        m.accepted_tags = 1;
        m.rejected_tags = 9;
        m.incorrect_verdicts = 9;
        let s = evaluate(&m, &R);
        assert!(s.can_vote);
        assert!(s.can_tag);
    }

    #[test]
    fn tag_grant_needs_volume_and_ratio() {
        let mut m = member(0, 0);
        m.accurate_verdicts = 25;
        assert!(evaluate(&m, &R).can_tag);
        m.accurate_verdicts = 24;
        assert!(!evaluate(&m, &R).can_tag);
        m.bonus_verdicts = 1;
        assert!(evaluate(&m, &R).can_tag);
        m.incorrect_verdicts = 2;
        assert!(evaluate(&m, &R).can_tag);
        m.incorrect_verdicts = 3;
        assert!(!evaluate(&m, &R).can_tag);
    }

    #[test]
    fn tag_eligibility_grants_voting() {
        let mut m = member(0, 0);
        m.accurate_verdicts = 25;
        m.rejected_tags = 3;
        assert!(evaluate(&m, &R).can_vote);
        assert!(explain(&m, &R).can_vote);
        m.accurate_verdicts = 0;
        m.tag_granted = true;
        assert!(evaluate(&m, &R).can_vote);
    }

    #[test]
    fn one_strike_blocks_both_fresh_grants_but_keeps_held_ones() {
        let mut m = member(0, 1);
        m.accepted_tags = 50;
        m.accurate_verdicts = 50;
        let s = evaluate(&m, &R);
        assert!(!s.can_vote);
        assert!(!s.can_tag);
        m.vote_granted = true;
        m.tag_granted = true;
        let s = evaluate(&m, &R);
        assert!(s.can_vote);
        assert!(s.can_tag);
    }

    #[test]
    fn two_strikes_revoke_everything() {
        let mut m = member(0, 2);
        m.accepted_tags = 50;
        m.accurate_verdicts = 50;
        m.vote_granted = true;
        m.tag_granted = true;
        let s = evaluate(&m, &R);
        assert!(!s.can_vote);
        assert!(!s.can_tag);
        assert_eq!(effective_level(&m, &R), -1);
    }

    #[test]
    fn fresh_evaluation_can_revoke_a_held_grant() {
        let mut m = member(0, 0);
        m.vote_granted = true;
        m.tag_granted = true;
        m.accepted_tags = 2;
        m.accurate_verdicts = 3;
        let s = evaluate_fresh(&m, &R);
        assert!(!s.can_vote);
        assert!(!s.can_tag);
        m.accepted_tags = 5;
        m.accurate_verdicts = 25;
        let s = evaluate_fresh(&m, &R);
        assert!(s.can_vote);
        assert!(s.can_tag);
    }

    #[test]
    fn effective_level_reflects_standing_not_rank() {
        let mut m = member(0, 0);
        assert_eq!(effective_level(&m, &R), 0);
        m.tag_granted = true;
        assert_eq!(effective_level(&m, &R), 1);
        m.access_level = 3;
        assert_eq!(effective_level(&m, &R), 3);
    }

    #[test]
    fn targets_fold_the_ratio_into_one_number() {
        let mut m = member(0, 0);
        assert_eq!(vote_target(&m, &R), 5);
        assert_eq!(tag_target(&m, &R), 25);
        m.rejected_tags = 1;
        m.incorrect_verdicts = 3;
        assert_eq!(vote_target(&m, &R), 12);
        assert_eq!(tag_target(&m, &R), 36);
    }

    type Setup = Box<dyn Fn(&mut Member)>;

    #[test]
    fn explain_matches_evaluate_across_scenarios() {
        let scenarios: Vec<(i64, Setup)> = vec![
            (0, Box::new(|m| m.accepted_tags = 4)),
            (0, Box::new(|m| m.accepted_tags = 5)),
            (
                0,
                Box::new(|m| {
                    m.vote_granted = true;
                    m.rejected_tags = 3;
                }),
            ),
            (0, Box::new(|m| m.accurate_verdicts = 25)),
            (0, Box::new(|m| m.accurate_verdicts = 24)),
            (1, Box::new(|m| m.accurate_verdicts = 30)),
            (2, Box::new(|m| m.tag_granted = true)),
        ];
        for (strikes, setup) in scenarios {
            let mut m = member(0, strikes);
            setup(&mut m);
            let evaluated = evaluate(&m, &R);
            let explanation = explain(&m, &R);
            assert_eq!(evaluated.can_vote, explanation.can_vote);
            assert_eq!(evaluated.can_tag, explanation.can_tag);
        }
    }

    #[test]
    fn explain_suspended_mentions_strike_count() {
        let explanation = explain(&member(0, 2), &R);
        assert!(explanation.vote_reason.contains("2 active strikes"));
        assert!(explanation.tag_reason.contains("2 active strikes"));
    }
}
