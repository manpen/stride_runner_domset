use std::{path::Path, sync::Arc};

use anyhow::Context;
use tokio::{fs::File, io::AsyncWriteExt, sync::Mutex};

use crate::commands::run::job::JobResult;

use super::IId;

pub struct RunSummaryLogger {
    // we are not using a BufWriter, since all writes are prepared and flushed
    file: Arc<Mutex<File>>,
}

const HEADER_STR: &str = "iid,time_sec,state,score,best_score_known\n";

impl RunSummaryLogger {
    pub async fn try_new(path: &Path) -> anyhow::Result<Self> {
        let mut file = File::create(path)
            .await
            .with_context(|| format!("Failed to create run summary file at {path:?}"))?;

        file.write_all(HEADER_STR.as_bytes()).await?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub async fn log_job_result(&self, iid: IId, summary: &JobResult) -> anyhow::Result<()> {
        use crate::commands::run::job::JobResultState::*;

        let (score, best_known) = match summary.state {
            BestKnown { score } => (Some(score), Some(score)),
            Suboptimal { score, best_known } => (Some(score), Some(best_known)),
            _ => (None, None),
        };

        let line = format!(
            "{},{},{},{},{}\n",
            iid.iid_to_u32(),
            summary.runtime.as_secs_f64(),
            summary.state,
            score.map_or_else(String::new, |s| s.to_string()),
            best_known.map_or_else(String::new, |s| s.to_string()),
        );

        let mut file = self.file.lock().await;
        file.write_all(line.as_bytes())
            .await
            .with_context(|| "Failed to write to run summary file")?;

        // we flush to avoid loss of data if the runner crashes
        file.flush()
            .await
            .with_context(|| "Failed to flush run summary file")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::commands::run::job::JobResultState;
    use tempdir::TempDir;

    use super::*;

    #[tokio::test]
    async fn logger() {
        let dir = TempDir::new("run_summary_logger").unwrap();
        let path = dir.path().join("summary.csv");

        let logger = RunSummaryLogger::try_new(&path).await.unwrap();

        {
            let job_result = JobResult {
                state: JobResultState::BestKnown { score: 42 },
                runtime: std::time::Duration::from_secs(1),
            };
            logger
                .log_job_result(IId::new(1), &job_result)
                .await
                .unwrap();
        }

        {
            let job_result = JobResult {
                state: JobResultState::Suboptimal {
                    score: 1337,
                    best_known: 1024,
                },
                runtime: std::time::Duration::from_secs(4),
            };
            logger
                .log_job_result(IId::new(2), &job_result)
                .await
                .unwrap();
        }

        {
            let job_result = JobResult {
                state: JobResultState::Error,
                runtime: std::time::Duration::from_secs(2),
            };
            logger
                .log_job_result(IId::new(3), &job_result)
                .await
                .unwrap();
        }

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(
            content,
            "iid,time_sec,state,score,best_score_known\n1,1,best,42,42\n2,4,suboptimal,1337,1024\n3,2,error,,\n"
        );
    }
}
