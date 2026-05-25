use crate::rules;

pub fn calculate_score(tag_names: &[&str]) -> f64 {
    tag_names
        .iter()
        .filter_map(|name| rules::lookup(name))
        .map(|def| def.score)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_sniper_tag() {
        assert_eq!(calculate_score(&["sniper"]), 10.0);
    }

    #[test]
    fn multiple_tags_sum_scores() {
        assert_eq!(calculate_score(&["sniper", "blatant_cheater"]), 15.0);
    }

    #[test]
    fn zero_score_tags() {
        assert_eq!(calculate_score(&["replays_needed"]), 0.0);
        assert_eq!(calculate_score(&["caution"]), 0.0);
    }

    #[test]
    fn unknown_tags_ignored() {
        assert_eq!(calculate_score(&["nonexistent", "sniper"]), 10.0);
    }

    #[test]
    fn empty_tags() {
        assert_eq!(calculate_score(&[]), 0.0);
    }

    #[test]
    fn confirmed_cheater_score() {
        assert_eq!(calculate_score(&["confirmed_cheater"]), 5.0);
    }
}
