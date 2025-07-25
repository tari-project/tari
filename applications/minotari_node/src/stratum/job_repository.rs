use async_trait::async_trait;
use uuid::Uuid;

use crate::stratum::job::Job;

#[async_trait]
pub trait JobRepository {
    async fn insert_job(&self, job: Job) -> Result<(), anyhow::Error>;
    async fn get_job(&self, job_id: Uuid) -> Result<Option<Job>, anyhow::Error>;
}
