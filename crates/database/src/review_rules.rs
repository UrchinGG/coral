use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

const CACHE_TTL: Duration = Duration::from_secs(30);
const MAX_VALUE: i64 = 1000;

static CACHE: LazyLock<RwLock<Option<(ReviewRules, Instant)>>> =
    LazyLock::new(|| RwLock::new(None));

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow, Serialize, Deserialize)]
pub struct ReviewRules {
    pub accept_threshold: i64,
    pub reject_threshold: i64,
    pub vote_grant_approvals: i64,
    pub tag_grant_correct: i64,
    pub earn_ratio: i64,
    pub bonus_weight: i64,
    pub revoke_strikes: i64,
}

impl ReviewRules {
    pub const DEFAULT: Self = Self {
        accept_threshold: 8,
        reject_threshold: 3,
        vote_grant_approvals: 5,
        tag_grant_correct: 25,
        earn_ratio: 12,
        bonus_weight: 5,
        revoke_strikes: 2,
    };

    pub fn suspends(&self, strikes: usize) -> bool {
        strikes as i64 >= self.revoke_strikes
    }

    pub fn validate(&self) -> Result<(), String> {
        let fields = [
            ("accept_threshold", self.accept_threshold),
            ("reject_threshold", self.reject_threshold),
            ("vote_grant_approvals", self.vote_grant_approvals),
            ("tag_grant_correct", self.tag_grant_correct),
            ("earn_ratio", self.earn_ratio),
            ("bonus_weight", self.bonus_weight),
            ("revoke_strikes", self.revoke_strikes),
        ];
        match fields.iter().find(|(_, v)| !(1..=MAX_VALUE).contains(v)) {
            Some((name, _)) => Err(format!("{name} must be between 1 and {MAX_VALUE}")),
            None => Ok(()),
        }
    }
}

const RULE_COLUMNS: &str = "accept_threshold, reject_threshold, vote_grant_approvals,
    tag_grant_correct, earn_ratio, bonus_weight, revoke_strikes";

pub struct ReviewRulesRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> ReviewRulesRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn current(&self) -> Result<ReviewRules, sqlx::Error> {
        if let Some((rules, at)) = *CACHE.read().expect("rules cache poisoned")
            && at.elapsed() < CACHE_TTL
        {
            return Ok(rules);
        }
        let rules = self.fetch().await?;
        remember(rules);
        Ok(rules)
    }

    pub async fn fetch(&self) -> Result<ReviewRules, sqlx::Error> {
        sqlx::query_as(&format!(
            "SELECT {RULE_COLUMNS} FROM review_rules WHERE id = 1"
        ))
        .fetch_one(self.pool)
        .await
    }

    pub async fn update(&self, rules: &ReviewRules, updated_by: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE review_rules SET
                 accept_threshold = $1, reject_threshold = $2, vote_grant_approvals = $3,
                 tag_grant_correct = $4, earn_ratio = $5, bonus_weight = $6,
                 revoke_strikes = $7, updated_at = NOW(), updated_by = $8
             WHERE id = 1",
        )
        .bind(rules.accept_threshold)
        .bind(rules.reject_threshold)
        .bind(rules.vote_grant_approvals)
        .bind(rules.tag_grant_correct)
        .bind(rules.earn_ratio)
        .bind(rules.bonus_weight)
        .bind(rules.revoke_strikes)
        .bind(updated_by)
        .execute(self.pool)
        .await?;
        remember(*rules);
        Ok(())
    }
}

fn remember(rules: ReviewRules) {
    *CACHE.write().expect("rules cache poisoned") = Some((rules, Instant::now()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_zero_is_not() {
        assert!(ReviewRules::DEFAULT.validate().is_ok());
        let rules = ReviewRules {
            earn_ratio: 0,
            ..ReviewRules::DEFAULT
        };
        assert_eq!(
            rules.validate(),
            Err("earn_ratio must be between 1 and 1000".to_string())
        );
    }

    #[test]
    fn suspension_follows_the_strike_limit() {
        assert!(!ReviewRules::DEFAULT.suspends(1));
        assert!(ReviewRules::DEFAULT.suspends(2));
    }
}
