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

pub fn strike_count(member: &Member) -> usize {
    member.strikes.as_array().map_or(0, Vec::len)
}

pub fn effective_level(member: &Member) -> i16 {
    if member.access_level >= AccessRank::Helper.to_level() {
        return member.access_level;
    }
    if strike_count(member) >= REVOKE_STRIKES {
        return -1;
    }
    if member.tag_granted { 1 } else { 0 }
}

pub fn evaluate(member: &Member) -> Standing {
    let strikes = strike_count(member);
    Standing {
        can_vote: eval_vote(member, strikes),
        can_tag: eval_tag(member, strikes),
    }
}

pub async fn refresh(
    repo: &MemberRepository<'_>,
    discord_id: i64,
) -> Result<StandingChange, sqlx::Error> {
    let Some(member) = repo.get_by_discord_id(discord_id).await? else {
        return Ok(StandingChange::default());
    };
    let standing = evaluate(&member);
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
            config: json!({}),
            strikes: json!(vec![json!({}); strikes]),
        }
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
}
