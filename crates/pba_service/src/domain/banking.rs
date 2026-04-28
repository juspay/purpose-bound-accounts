use std::fmt;

use serde::Serialize;

/// Indian Financial System Code — 11 chars, `^[A-Z]{4}0[A-Z0-9]{6}$`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct Ifsc(String);

/// Indian bank account number — 9 to 18 digits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct AccountNumber(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankingValidationError {
    InvalidIfsc(String),
    InvalidAccountNumber(String),
}

impl fmt::Display for BankingValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIfsc(reason) => write!(f, "Invalid IFSC: {reason}"),
            Self::InvalidAccountNumber(reason) => write!(f, "Invalid account number: {reason}"),
        }
    }
}

/// Validates the IFSC shape: 11 chars, `[A-Z]{4}` then `0` then `[A-Z0-9]{6}`.
fn is_valid_ifsc(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 11 {
        return false;
    }
    if !bytes[..4].iter().all(|b| b.is_ascii_uppercase()) {
        return false;
    }
    if bytes[4] != b'0' {
        return false;
    }
    bytes[5..]
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

impl Ifsc {
    pub fn parse(s: &str) -> Result<Self, BankingValidationError> {
        let trimmed = s.trim();
        if !is_valid_ifsc(trimmed) {
            return Err(BankingValidationError::InvalidIfsc(format!(
                "must be 11 chars matching ^[A-Z]{{4}}0[A-Z0-9]{{6}}$, got {trimmed:?}"
            )));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Ifsc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validates that the string is 9-18 ASCII digits.
fn is_valid_account_number(s: &str) -> bool {
    let bytes = s.as_bytes();
    (9..=18).contains(&bytes.len()) && bytes.iter().all(|b| b.is_ascii_digit())
}

impl AccountNumber {
    pub fn parse(s: &str) -> Result<Self, BankingValidationError> {
        let trimmed = s.trim();
        if !is_valid_account_number(trimmed) {
            return Err(BankingValidationError::InvalidAccountNumber(format!(
                "must be 9-18 digits, got {} char(s)",
                trimmed.chars().count()
            )));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ifsc_accepts_valid() {
        assert!(Ifsc::parse("HDFC0001234").is_ok());
        assert!(Ifsc::parse("SBIN0ABC123").is_ok());
    }

    #[test]
    fn ifsc_rejects_lowercase() {
        assert!(Ifsc::parse("hdfc0001234").is_err());
    }

    #[test]
    fn ifsc_rejects_wrong_fifth_char() {
        assert!(Ifsc::parse("HDFC1001234").is_err());
    }

    #[test]
    fn ifsc_rejects_wrong_length() {
        assert!(Ifsc::parse("HDFC000123").is_err());
        assert!(Ifsc::parse("HDFC00012345").is_err());
    }

    #[test]
    fn ifsc_trims_whitespace() {
        assert_eq!(
            Ifsc::parse("  HDFC0001234 ").unwrap().as_str(),
            "HDFC0001234"
        );
    }

    #[test]
    fn account_number_accepts_9_to_18_digits() {
        assert!(AccountNumber::parse("123456789").is_ok());
        assert!(AccountNumber::parse("123456789012345678").is_ok());
    }

    #[test]
    fn account_number_rejects_too_short_or_long() {
        assert!(AccountNumber::parse("12345678").is_err());
        assert!(AccountNumber::parse("1234567890123456789").is_err());
    }

    #[test]
    fn account_number_rejects_non_digits() {
        assert!(AccountNumber::parse("12345abc9").is_err());
        assert!(AccountNumber::parse("12345-789").is_err());
    }

    #[test]
    fn account_number_preserves_leading_zeros() {
        assert_eq!(
            AccountNumber::parse("000123456").unwrap().as_str(),
            "000123456"
        );
    }
}
