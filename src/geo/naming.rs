//! The published name of a node.
//!
//! The shape is fixed by the project brief:
//!
//! ```text
//! <flag> <English country name> - Netherite - <four digits>
//! ```
//!
//! The country is the *measured* exit, not the one in the source's label. A
//! node advertised as Germany that answers from the Netherlands is a Dutch node,
//! and a subscription that says otherwise is lying in the one field a user
//! reads before connecting.

use super::countries::COUNTRIES;
use super::flags::flag;

/// English name for an ISO code, or the code itself if the table has no row.
pub fn country_name(code: Option<&str>) -> String {
    let Some(code) = code else {
        return "Unknown".to_string();
    };
    if code.eq_ignore_ascii_case("unknown") {
        return "Unknown".to_string();
    }
    COUNTRIES
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(code))
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| code.to_ascii_uppercase())
}

/// Build the exact published name.
pub fn published_name(code: Option<&str>, digits: u16, name: &str) -> String {
    format!("{} {} - {name} - {digits}", flag(code), country_name(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_is_exact() {
        assert_eq!(
            published_name(Some("NL"), 4821, "Netherite"),
            "🇳🇱 Netherlands - Netherite - 4821"
        );
    }

    #[test]
    fn the_name_is_the_one_asked_for() {
        assert_eq!(
            published_name(Some("NL"), 4821, "Rock"),
            "🇳🇱 Netherlands - Rock - 4821"
        );
    }

    #[test]
    fn unknown_country_is_spelled_out() {
        assert_eq!(
            published_name(None, 1000, "Netherite"),
            "🌐 Unknown - Netherite - 1000"
        );
    }

    #[test]
    fn an_unlisted_code_falls_back_to_the_code() {
        assert_eq!(country_name(Some("ZZ")), "ZZ");
    }
}
