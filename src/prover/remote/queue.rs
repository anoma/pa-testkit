//! Polling helpers for the remote proving queue.

use std::time::Duration;

use heliax_ap_orchestrator_sdk::JobStatus;
use heliax_ap_orchestrator_sdk::QueueClient;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const POLL_TIMEOUT: Duration = Duration::from_secs(600);
const TRANSIENT_ERROR_GRACE: Duration = Duration::from_secs(30);

pub(super) async fn poll_until_done(queue: &QueueClient, job_id: &str) -> anyhow::Result<()> {
    let start = tokio::time::Instant::now();
    let deadline = start + POLL_TIMEOUT;
    loop {
        match queue.get_job_status(job_id.to_string()).await {
            Ok(JobStatus::Success) => return Ok(()),
            Ok(JobStatus::Failed) => {
                anyhow::bail!("queue job {job_id} failed");
            }
            Ok(JobStatus::DeadLetter) => {
                anyhow::bail!("queue job {job_id} entered dead letter");
            }
            Ok(JobStatus::Queued | JobStatus::Running) => {}
            Err(err) => {
                if start.elapsed() > TRANSIENT_ERROR_GRACE {
                    anyhow::bail!("polling queue job {job_id}: {err}");
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("queue job {job_id} did not complete within {POLL_TIMEOUT:?}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

pub(super) async fn fetch_job_result<T: serde::de::DeserializeOwned>(
    queue: &QueueClient,
    job_id: &str,
) -> anyhow::Result<T> {
    poll_until_done(queue, job_id).await?;

    let start = tokio::time::Instant::now();
    loop {
        match queue.get_job_result::<T>(job_id.to_string()).await {
            Ok(Some(result)) => return Ok(result),
            Ok(None) => {}
            Err(err) => {
                if start.elapsed() > TRANSIENT_ERROR_GRACE {
                    anyhow::bail!("fetching queue job {job_id} result: {err}");
                }
            }
        }
        if start.elapsed() > TRANSIENT_ERROR_GRACE {
            anyhow::bail!("queue job {job_id} reported Done but never returned a result");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
