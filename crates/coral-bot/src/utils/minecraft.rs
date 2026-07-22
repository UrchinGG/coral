pub fn format_uuid_dashed(uuid: &str) -> String {
    if uuid.len() != 32 {
        return uuid.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &uuid[0..8],
        &uuid[8..12],
        &uuid[12..16],
        &uuid[16..20],
        &uuid[20..32]
    )
}

pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',')
        }
        result.insert(0, c);
    }
    result
}

fn is_escaped_in_reason(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '*' | '_' | '~' | '`' | '|' | '>' | '#' | '[' | ']' | '(' | ')' | '-' | '@' | '<'
    )
}

pub fn sanitize_reason(reason: &str) -> String {
    let mut out = String::with_capacity(reason.len());
    for ch in reason.chars() {
        if is_escaped_in_reason(ch) {
            out.push('\\');
        } else if matches!(ch, '\n' | '\r') {
            out.push(' ');
            continue;
        }
        out.push(ch);
    }
    out
}

/// Reverses [`sanitize_reason`].
///
/// Reasons are read back out of already rendered messages in the review and
/// evidence flows; without this the escapes stack up on every redraw.
pub fn unsanitize_reason(reason: &str) -> String {
    let mut out = String::with_capacity(reason.len());
    let mut chars = reason.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek().copied().is_some_and(is_escaped_in_reason) {
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_round_trips() {
        for reason in [
            "autoclutch (temp tag, i will change later)",
            "uses \\ backslash * and _underscores_",
            "plain reason",
        ] {
            assert_eq!(unsanitize_reason(&sanitize_reason(reason)), reason);
        }
    }

    #[test]
    fn re_rendering_does_not_stack_escapes() {
        let raw = "autoclutch (temp tag)";
        let once = sanitize_reason(raw);
        let twice = sanitize_reason(&unsanitize_reason(&once));
        assert_eq!(once, twice);
    }
}

pub fn format_tag_detail(tag: &database::PlayerEvent) -> String {
    if tag.tag_type.as_deref() == Some("replays_needed") {
        return match tag.expires_at {
            Some(ts) => format!("Expires <t:{}:R>", ts.timestamp()),
            None => "No expiration".into(),
        };
    }
    sanitize_reason(tag.reason.as_deref().unwrap_or(""))
}

pub fn generate_api_key() -> String {
    uuid::Uuid::new_v4().to_string()
}
