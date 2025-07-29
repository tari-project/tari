use dashmap::DashMap;
use log::debug;
use uuid::Uuid;

use crate::stratum::{job::Job, job_repository::JobRepository};

const LOG_TARGET: &str = "minotari::base_node::stratum::memory_job_repository";
#[derive(Default)]
pub(crate) struct MemoryJobRepository {
    jobs: DashMap<String, Job>,
}

#[async_trait::async_trait]
impl JobRepository for MemoryJobRepository {
    async fn insert_job(&self, job: Job) -> Result<(), anyhow::Error> {
        debug!(target: LOG_TARGET, "Inserting job with ID: {}", job.id);
        self.jobs.insert(job.job_id.clone(), job);
        Ok(())
    }

    async fn get_job(&self, job_id: String) -> Result<Option<Job>, anyhow::Error> {
        debug!(target: LOG_TARGET, "Getting job with ID: {}", job_id);
        Ok(self.jobs.get(&job_id).map(|r| r.clone()))
    }
}
