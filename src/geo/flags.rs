//! Flag emoji from an ISO code.
//!
//! A flag is two regional-indicator symbols, and a regional indicator is
//! `U+1F1E6 + (letter - 'A')`. No table is needed, and no table can go stale:
//! the same arithmetic covers every assigned code.

/// `NL` becomes 🇳🇱; anything that is not two ASCII letters becomes 🌐.
///
/// The fallback is deliberate. A broken flag renders as two empty boxes in some
/// clients, which looks like a bug in the subscription; the globe reads as
/// "country unknown", which is what it is.
pub fn flag(code: Option<&str>) -> String {
    let Some(code) = code else {
        return "🌐".to_string();
    };
    let mut chars = code.chars();
    let (Some(a), Some(b), None) = (chars.next(), chars.next(), chars.next()) else {
        return "🌐".to_string();
    };
    if !a.is_ascii_alphabetic() || !b.is_ascii_alphabetic() {
        return "🌐".to_string();
    }
    let a = regional(a);
    let b = regional(b);
    format!("{a}{b}")
}

fn regional(letter: char) -> char {
    let offset = letter.to_ascii_uppercase() as u32 - 'A' as u32;
    char::from_u32(0x1F1E6 + offset).unwrap_or('🌐')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_a_flag() {
        assert_eq!(flag(Some("NL")), "🇳🇱");
        assert_eq!(flag(Some("de")), "🇩🇪");
    }

    #[test]
    fn unknown_is_a_globe() {
        assert_eq!(flag(None), "🌐");
        assert_eq!(flag(Some("X")), "🌐");
        assert_eq!(flag(Some("123")), "🌐");
    }
}
