use std::borrow::Cow;

const NUL: char = '\0';
const NUL_REPLACEMENT: &str = "\u{FFFD}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredText {
    pub text: String,
    pub exact: bool,
}

impl StoredText {
    pub fn decode(bytes: &[u8]) -> Self {
        let decoded = String::from_utf8_lossy(bytes);
        let valid = matches!(decoded, Cow::Borrowed(_));
        match decoded.contains(NUL) {
            true => Self::rebuilt(decoded.replace(NUL, NUL_REPLACEMENT)),
            false if valid => Self {
                text: decoded.into_owned(),
                exact: true,
            },
            false => Self::rebuilt(decoded.into_owned()),
        }
    }

    pub fn truncated(self, limit: usize) -> Self {
        if self.text.len() <= limit {
            return self;
        }
        let mut text = self.text;
        text.truncate(text.floor_char_boundary(limit));
        Self::rebuilt(text)
    }

    fn rebuilt(mut text: String) -> Self {
        text.shrink_to_fit();
        Self { text, exact: false }
    }
}

pub fn without_nul(text: String) -> String {
    match text.contains(NUL) {
        true => text.replace(NUL, NUL_REPLACEMENT),
        false => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_text_is_kept_exactly() {
        let stored = StoredText::decode(br#"{"uuids":[]}"#);
        assert_eq!(stored.text, r#"{"uuids":[]}"#);
        assert!(stored.exact);
    }

    #[test]
    fn nul_bytes_are_replaced_and_marked_inexact() {
        let stored = StoredText::decode(b"a\0b");
        assert_eq!(stored.text, "a\u{FFFD}b");
        assert!(!stored.exact);
        assert_eq!(without_nul("x\0".into()), "x\u{FFFD}");
        assert_eq!(without_nul("plain".into()), "plain");
    }

    #[test]
    fn invalid_utf8_is_lossy_and_inexact() {
        let stored = StoredText::decode(&[b'a', 0xFF]);
        assert_eq!(stored.text, "a\u{FFFD}");
        assert!(!stored.exact);
    }

    #[test]
    fn decoded_growth_is_cut_back_on_a_char_boundary() {
        let bytes = [0xFF; 10];
        let stored = StoredText::decode(&bytes).truncated(bytes.len());
        assert_eq!(stored.text, "\u{FFFD}".repeat(3));
        assert_eq!(stored.text.capacity(), stored.text.len());
        assert!(!stored.exact);

        let stored = StoredText::decode(b"short").truncated(64);
        assert_eq!(stored.text, "short");
        assert!(stored.exact);
    }
}
