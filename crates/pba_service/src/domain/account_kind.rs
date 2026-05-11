use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum AccountKind {
    Pb,
    Normal,
}

#[allow(dead_code)]
impl AccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pb => "pb",
            Self::Normal => "normal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pb" => Some(Self::Pb),
            "normal" => Some(Self::Normal),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_strings() {
        assert_eq!(AccountKind::Pb.as_str(), "pb");
        assert_eq!(AccountKind::Normal.as_str(), "normal");
        assert_eq!(AccountKind::from_str("pb"), Some(AccountKind::Pb));
        assert_eq!(AccountKind::from_str("normal"), Some(AccountKind::Normal));
        assert_eq!(AccountKind::from_str("other"), None);
    }
}
