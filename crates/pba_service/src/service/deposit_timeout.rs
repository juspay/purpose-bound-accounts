use std::sync::Arc;
use std::time::Duration;

use crate::domain::transaction::TransactionStatus;
use crate::repository::transaction_repo::TransactionRepo;

pub async fn run_deposit_timeout_poller(
    transaction_repo: Arc<TransactionRepo>,
    poll_interval_seconds: u64,
) {
    let interval = Duration::from_secs(poll_interval_seconds);
    tracing::info!(poll_interval_seconds, "Starting deposit timeout poller");

    loop {
        tokio::time::sleep(interval).await;

        match transaction_repo.find_timed_out_pending().await {
            Ok(timed_out) => {
                // Track which correlation_ids we've already handled this cycle
                // so we don't update the same pair twice (both legs will be
                // returned by find_timed_out_pending).
                let mut handled_correlations = std::collections::HashSet::new();

                for txn in timed_out {
                    // For transfer pairs, void both legs atomically by correlation_id.
                    // For solo deposits (correlation_id IS NULL), use the per-row update.
                    if let Some(correlation_id) = txn.correlation_id {
                        if !handled_correlations.insert(correlation_id) {
                            // Already handled this pair via the other leg in this cycle.
                            continue;
                        }
                        match sqlx::query(
                            r#"UPDATE transactions
                               SET status = 'voided', updated_at = now()
                               WHERE correlation_id = $1 AND status = 'pending'"#,
                        )
                        .bind(correlation_id)
                        .execute(transaction_repo.pool())
                        .await
                        {
                            Ok(result) => {
                                tracing::warn!(
                                    correlation_id = %correlation_id,
                                    rows_voided = result.rows_affected(),
                                    "Pending transfer timed out and voided (both legs)"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    correlation_id = %correlation_id,
                                    error = %e,
                                    "Failed to void timed-out transfer legs"
                                );
                            }
                        }
                    } else {
                        // Solo deposit — use the existing per-row update.
                        match transaction_repo
                            .update_status(txn.id, TransactionStatus::Voided)
                            .await
                        {
                            Ok(_) => {
                                tracing::warn!(
                                    transaction_id = %txn.id,
                                    account_id = %txn.account_id,
                                    gateway_ref = txn.gateway_ref.as_deref().unwrap_or("none"),
                                    amount = txn.amount,
                                    "Pending deposit timed out and voided"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    transaction_id = %txn.id,
                                    error = %e,
                                    "Failed to update timed-out deposit status"
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to query timed-out deposits");
            }
        }
    }
}
