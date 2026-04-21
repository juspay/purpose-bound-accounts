#![cfg(feature = "aws-kms")]

use async_trait::async_trait;
use aws_sdk_kms::primitives::Blob;
use base64::Engine;

use crate::secrets::{SecretsError, SecretsProvider};

/// AWS KMS-based secrets provider. Decrypts base64-encoded KMS ciphertext.
pub struct AwsKmsProvider {
    client: aws_sdk_kms::Client,
}

impl AwsKmsProvider {
    pub async fn new() -> Self {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_kms::Client::new(&config);
        Self { client }
    }
}

#[async_trait]
impl SecretsProvider for AwsKmsProvider {
    async fn decrypt(&self, value: &str) -> Result<String, SecretsError> {
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|e| SecretsError::DecryptionFailed(format!("base64 decode failed: {e}")))?;

        let response = self
            .client
            .decrypt()
            .ciphertext_blob(Blob::new(ciphertext))
            .send()
            .await
            .map_err(|e| SecretsError::DecryptionFailed(format!("KMS decrypt failed: {e}")))?;

        let plaintext = response
            .plaintext()
            .ok_or_else(|| SecretsError::DecryptionFailed("KMS returned no plaintext".into()))?;

        String::from_utf8(plaintext.as_ref().to_vec())
            .map_err(|e| SecretsError::DecryptionFailed(format!("plaintext is not UTF-8: {e}")))
    }
}
