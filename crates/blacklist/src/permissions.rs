const MEMBER: i16 = 1;
const HELPER: i16 = 2;

const WINDOW_MINUTES: i64 = 30;

pub fn can_add(tag_type: &str, level: i16) -> bool {
    match tag_type {
        "sniper" => level >= 0,
        "blatant_cheater" | "closet_cheater" => level >= MEMBER,
        "replays_needed" | "caution" => level >= HELPER,
        _ => false,
    }
}

pub fn can_remove(_tag_type: &str, level: i16, is_own: bool, age_minutes: i64) -> bool {
    if level >= HELPER {
        return true;
    }
    is_own && age_minutes <= WINDOW_MINUTES
}

pub fn can_overwrite(old_tag_type: &str, level: i16, is_own: bool, age_minutes: i64) -> bool {
    if level >= HELPER {
        return true;
    }
    if level >= MEMBER {
        return old_tag_type != "confirmed_cheater" && old_tag_type != "caution";
    }
    is_own && age_minutes <= WINDOW_MINUTES
}

pub fn can_modify(tag_type: &str, level: i16, is_own: bool, age_minutes: i64) -> bool {
    can_remove(tag_type, level, is_own, age_minutes)
}

pub fn can_change_to(new_type: &str, level: i16) -> bool {
    can_add(new_type, level)
}

pub fn can_set_hide(level: i16) -> bool {
    level >= HELPER
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRUCK: i16 = -1;
    const DEFAULT: i16 = 0;
    const TRUSTED: i16 = MEMBER;
    const MODERATOR: i16 = 3;

    #[test]
    fn trusted_overwrites_another_users_tag() {
        for old in [
            "sniper",
            "blatant_cheater",
            "closet_cheater",
            "replays_needed",
        ] {
            assert!(
                can_overwrite(old, TRUSTED, false, 10_000),
                "trusted should overwrite {old} regardless of author or age"
            );
        }
    }

    #[test]
    fn trusted_cannot_overwrite_protected_tags() {
        assert!(!can_overwrite("confirmed_cheater", TRUSTED, false, 0));
        assert!(!can_overwrite("caution", TRUSTED, false, 0));
        assert!(!can_overwrite("confirmed_cheater", TRUSTED, true, 0));
        assert!(!can_overwrite("caution", TRUSTED, true, 0));
    }

    #[test]
    fn moderator_overwrites_anything() {
        assert!(can_overwrite("confirmed_cheater", MODERATOR, false, 10_000));
        assert!(can_overwrite("caution", MODERATOR, false, 10_000));
    }

    #[test]
    fn helper_handles_protected_tags() {
        for tag in ["confirmed_cheater", "caution"] {
            assert!(can_overwrite(tag, HELPER, false, 10_000));
            assert!(can_remove(tag, HELPER, false, 10_000));
        }
        assert!(can_add("caution", HELPER));
        assert!(can_set_hide(HELPER));
    }

    #[test]
    fn members_stay_below_helper_permissions() {
        assert!(!can_add("caution", TRUSTED));
        assert!(!can_set_hide(TRUSTED));
    }

    #[test]
    fn untrusted_overwrites_only_own_recent_tag() {
        assert!(can_overwrite("sniper", DEFAULT, true, 10));
        assert!(!can_overwrite("sniper", DEFAULT, false, 10));
        assert!(!can_overwrite("sniper", DEFAULT, true, WINDOW_MINUTES + 1));
    }

    #[test]
    fn struck_users_cannot_add_or_touch_others_tags() {
        assert!(!can_add("sniper", STRUCK));
        assert!(!can_overwrite("sniper", STRUCK, false, 0));
        assert!(!can_remove("sniper", STRUCK, false, 0));
    }

    #[test]
    fn struck_users_retain_self_cleanup() {
        assert!(can_remove("sniper", STRUCK, true, 0));
        assert!(can_overwrite("sniper", STRUCK, true, 0));
        assert!(!can_add("sniper", STRUCK));
    }

    #[test]
    fn trusted_removal_stays_narrower_than_overwrite() {
        assert!(can_overwrite("sniper", TRUSTED, false, 10));
        assert!(!can_remove("sniper", TRUSTED, false, 10));
        assert!(can_remove("sniper", TRUSTED, true, 10));
        assert!(!can_remove("sniper", TRUSTED, true, WINDOW_MINUTES + 1));
    }
}
