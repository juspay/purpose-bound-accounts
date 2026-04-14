use std::sync::Arc;
use std::time::Duration;

use crate::domain::deposit::DepositStatus;
use crate::repository::deposit_repo::DepositRepo;

pub async fn run_deposit_timeout_poller(
    deposit_repo: Arc<DepositRepo>,
    poll_interval_seconds: u64,
) {
    let interval = Duration::from_secs(poll_interval_seconds);
    tracing::info!(
        poll_interval_seconds,
        "Starting deposit timeout poller"
    );

    loop {
        tokio::time::sleep(interval).await;

        match deposit_repo.find_timed_out_pending().await {
            Ok(timed_out) => {
                for deposit in timed_out {
                    // TigerBeetle has already auto-voided. Just update PG.
                    match deposit_repo
                        .update_status(deposit.id, DepositStatus::Voided)
                        .await
                    {
                        Ok(_) => {
                            tracing::warn!(
                                deposit_id = %deposit.id,
                                account_id = %deposit.account_id,
                                gateway_ref = deposit.gateway_ref.as_deref().unwrap_or("none"),
                                amount = deposit.amount,
                                "Pending deposit timed out and voided"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                deposit_id = %deposit.id,
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
