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
                for txn in timed_out {
                    // TigerBeetle has already auto-voided. Just update PG.
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
            Err(e) => {
                tracing::error!(error = %e, "Failed to query timed-out deposits");
            }
        }
    }
}
