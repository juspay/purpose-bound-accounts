use std::fmt;

/// Errors that can occur during secret decryption.
#[derive(Debug)]
pub enum SecretsError {
    DecryptionFailed(String),
    ProviderInitFailed(String),
}

impl fmt::Display for SecretsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretsError::DecryptionFailed(msg) => write!(f, "decryption failed: {msg}"),
            SecretsError::ProviderInitFailed(msg) => {
                write!(f, "provider initialization failed: {msg}")
            }
        }
    }
}

impl std::error::Error for SecretsError {}

/// Trait for decrypting secret values at startup.
#[async_trait::async_trait]
pub trait SecretsProvider: Send + Sync {
    async fn decrypt(&self, value: &str) -> Result<String, SecretsError>;
}

/// Creates the appropriate secrets provider based on the `SECRETS_PROVIDER` env var.
///
/// Defaults to `"plaintext"` if unset.
pub async fn create_provider() -> Box<dyn SecretsProvider> {
    let provider_name =
        std::env::var("SECRETS_PROVIDER").unwrap_or_else(|_| "plaintext".to_string());

    match provider_name.as_str() {
        "plaintext" => Box::new(super::secrets_plaintext::PlaintextProvider),
        #[cfg(feature = "aws-kms")]
        "aws-kms" => Box::new(super::secrets_kms::AwsKmsProvider::new().await),
        other => panic!("unknown SECRETS_PROVIDER: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_provider_defaults_to_plaintext() {
        std::env::remove_var("SECRETS_PROVIDER");
        let provider = create_provider().await;
        let result = provider.decrypt("hello").await.unwrap();
        assert_eq!(result, "hello");
    }
}
