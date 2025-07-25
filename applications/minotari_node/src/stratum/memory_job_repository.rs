use dashmap::DashMap;
use uuid::Uuid;

use crate::stratum::{job::Job, job_repository::JobRepository};

#[derive(Default)]
pub(crate) struct MemoryJobRepository {
    jobs: DashMap<Uuid, Job>,
}

#[async_trait::async_trait]
impl JobRepository for MemoryJobRepository {
    async fn insert_job(&self, job: Job) -> Result<(), anyhow::Error> {
        self.jobs.insert(job.id, job);
        Ok(())
    }

    async fn get_job(&self, job_id: Uuid) -> Result<Option<Job>, anyhow::Error> {
        Ok(self.jobs.get(&job_id).map(|r| r.clone()))
    }
}
