use crate::secrets::{SecretsError, SecretsProvider};

/// Returns values as-is with no decryption. Used for local development.
pub struct PlaintextProvider;

#[async_trait::async_trait]
impl SecretsProvider for PlaintextProvider {
    async fn decrypt(&self, value: &str) -> Result<String, SecretsError> {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_value_unchanged() {
        let provider = PlaintextProvider;
        let result = provider.decrypt("my-secret-value").await.unwrap();
        assert_eq!(result, "my-secret-value");
    }
}
