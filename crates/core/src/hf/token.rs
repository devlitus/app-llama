//! Token de acceso de Hugging Face que nunca se muestra ni se registra.
use std::fmt;

use super::error::HfError;

const PREFIX: &str = "hf_";
const MIN_BODY_LEN: usize = 30;
const REDACTED: &str = "hf_***";

#[derive(Clone)]
pub struct HfToken(String);

impl HfToken {
    /// Equivale a `^hf_[A-Za-z0-9]{30,}$` tras recortar los espacios.
    pub fn parse(raw: &str) -> Result<HfToken, HfError> {
        let trimmed = raw.trim();
        let valid = trimmed.strip_prefix(PREFIX).is_some_and(|body| {
            body.len() >= MIN_BODY_LEN && body.chars().all(|c| c.is_ascii_alphanumeric())
        });
        if valid {
            Ok(HfToken(trimmed.to_owned()))
        } else {
            Err(HfError::InvalidToken)
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HfToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl fmt::Display for HfToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_token() -> String {
        format!("hf_{}", "a1B2".repeat(9))
    }

    fn assert_invalid(raw: &str) {
        assert!(matches!(HfToken::parse(raw), Err(HfError::InvalidToken)));
    }

    #[test]
    fn token_parse_valid() {
        assert!(HfToken::parse(&raw_token()).is_ok());
    }

    #[test]
    fn token_parse_trims_whitespace() {
        let token = HfToken::parse(&format!("  {}  \n", raw_token())).unwrap();
        assert_eq!(token.expose(), raw_token());
    }

    #[test]
    fn token_parse_wrong_prefix() {
        assert_invalid(&format!("ghp_{}", "a".repeat(34)));
    }

    #[test]
    fn token_parse_too_short() {
        assert_invalid("hf_abc");
    }

    #[test]
    fn token_parse_inner_space() {
        assert_invalid(&format!("hf_abc def{}", "a".repeat(30)));
    }

    #[test]
    fn token_parse_empty() {
        assert_invalid("");
    }

    #[test]
    fn token_debug_is_redacted() {
        let token = HfToken::parse(&raw_token()).unwrap();
        let shown = format!("{token:?}");
        assert_eq!(shown, "hf_***");
        assert!(!shown.contains(&raw_token()));
    }

    #[test]
    fn token_display_is_redacted() {
        let token = HfToken::parse(&raw_token()).unwrap();
        assert_eq!(format!("{token}"), "hf_***");
    }
}
