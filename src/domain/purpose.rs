use serde::Serialize;

/// A purpose type with its allowed MCCs.
#[derive(Debug, Clone, Serialize)]
pub struct PurposeType {
    pub purpose_code: String,
    pub allowed_mccs: Vec<MccEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MccEntry {
    pub mcc: String,
    pub description: Option<String>,
}

impl PurposeType {
    /// Check if a given MCC is allowed under this purpose.
    pub fn is_mcc_allowed(&self, mcc: &str) -> bool {
        self.allowed_mccs.iter().any(|entry| entry.mcc == mcc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health_purpose() -> PurposeType {
        PurposeType {
            purpose_code: "health".to_string(),
            allowed_mccs: vec![
                MccEntry {
                    mcc: "5912".to_string(),
                    description: Some("Pharmacies".to_string()),
                },
                MccEntry {
                    mcc: "8011".to_string(),
                    description: Some("Doctors".to_string()),
                },
            ],
        }
    }

    #[test]
    fn mcc_allowed() {
        let purpose = health_purpose();
        assert!(purpose.is_mcc_allowed("5912"));
        assert!(purpose.is_mcc_allowed("8011"));
    }

    #[test]
    fn mcc_not_allowed() {
        let purpose = health_purpose();
        assert!(!purpose.is_mcc_allowed("5411")); // grocery — not health
    }
}
