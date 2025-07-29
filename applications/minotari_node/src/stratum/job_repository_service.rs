use async_trait::async_trait;
use log::{debug, info};
use tari_shutdown::ShutdownSignal;
use tokio::{select, sync::oneshot};
use uuid::Uuid;

use crate::stratum::{job::Job, job_repository::JobRepository, JobRepositoryReceiver, JobRepositorySender};

const LOG_TARGET: &str = "minotari::base_node::stratum::job_repository_service";

pub(crate) struct JobRepositoryService<T: JobRepository> {
    job_repository: T,
    rx: JobRepositoryReceiver,
}

impl<T: JobRepository> JobRepositoryService<T> {
    pub(crate) fn new(job_repository: T, rx: JobRepositoryReceiver) -> Self {
        Self { job_repository, rx }
    }

    pub async fn start(mut self, mut shutdown: ShutdownSignal) -> Result<(), anyhow::Error> {
        loop {
            select! {
                _ = shutdown.wait() => {
                    println!("Shutting down Job Repository Service");
                    break;
                }
                Some(request) = self.rx.recv() => {
                    match request {
                        JobRepositoryRequest::InsertJob(job, responder) => {
                            debug!(target: "job_repository", "Inserting job with ID: {}", job.id);
                            let result = self.job_repository.insert_job(job).await;
                            let _ = responder.send(result);
                        }
                        JobRepositoryRequest::GetJob(id, responder) => {
                            debug!(target: "job_repository", "Getting job with ID: {}", id);
                            let result = self.job_repository.get_job(id).await;
                            let _ = responder.send(result);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

pub enum JobRepositoryRequest {
    InsertJob(Job, oneshot::Sender<Result<(), anyhow::Error>>),
    GetJob(String, oneshot::Sender<Result<Option<Job>, anyhow::Error>>),
}

#[derive(Clone)]
pub(crate) struct JobRepositoryClient {
    tx: JobRepositorySender,
}
impl JobRepositoryClient {
    pub fn new(tx: JobRepositorySender) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl JobRepository for JobRepositoryClient {
    async fn insert_job(&self, job: Job) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(JobRepositoryRequest::InsertJob(job, tx))
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send InsertJob request"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Failed to receive InsertJob response"))?
    }

    async fn get_job(&self, id: String) -> Result<Option<Job>, anyhow::Error> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(JobRepositoryRequest::GetJob(id, tx))
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send GetJob request"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Failed to receive GetJob response"))?
    }
}
