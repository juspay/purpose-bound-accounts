//! Boundary validation shared by the API handlers.

use crate::error::AppError;

/// Longest free-text value accepted on any request field. The `description` and
/// `void_reason` columns are unbounded `TEXT`, so this is the only place the
/// limit is enforced — keep every handler going through here rather than
/// re-stating the number.
pub const MAX_TEXT_LEN: usize = 256;

/// Reject an optional free-text field that exceeds [`MAX_TEXT_LEN`].
///
/// `field` names the request field in the error message so a caller sending
/// both a description and a reason can tell which one was rejected. Counts
/// characters, not bytes, so a non-ASCII value is not penalised for its UTF-8
/// encoding.
pub fn validate_text_length(field: &str, value: Option<&str>) -> Result<(), AppError> {
    match value {
        Some(v) if v.chars().count() > MAX_TEXT_LEN => Err(AppError::Validation(format!(
            "{field} must be \u{2264} {MAX_TEXT_LEN} characters"
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_absent_and_short_values() {
        assert!(validate_text_length("description", None).is_ok());
        assert!(validate_text_length("description", Some("")).is_ok());
        assert!(validate_text_length("description", Some("Salary credit")).is_ok());
    }

    #[test]
    fn accepts_exactly_the_limit() {
        let at_limit = "x".repeat(MAX_TEXT_LEN);
        assert!(validate_text_length("description", Some(&at_limit)).is_ok());
    }

    #[test]
    fn rejects_one_over_the_limit() {
        let over = "x".repeat(MAX_TEXT_LEN + 1);
        let err = validate_text_length("description", Some(&over)).expect_err("expected rejection");
        assert!(
            matches!(err, AppError::Validation(ref m) if m.contains("description")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn counts_characters_not_utf8_bytes() {
        // 256 multi-byte characters is 768 bytes but still within the limit.
        let multibyte = "\u{20b9}".repeat(MAX_TEXT_LEN);
        assert!(multibyte.len() > MAX_TEXT_LEN);
        assert!(validate_text_length("description", Some(&multibyte)).is_ok());
    }

    #[test]
    fn names_the_offending_field() {
        let over = "x".repeat(MAX_TEXT_LEN + 1);
        let err = validate_text_length("reason", Some(&over)).expect_err("expected rejection");
        assert!(
            matches!(err, AppError::Validation(ref m) if m.contains("reason")),
            "unexpected error: {err:?}"
        );
    }
}
