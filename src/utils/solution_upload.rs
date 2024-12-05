use derive_builder::Builder;
use serde::Serialize;
use tracing::{debug, trace};

use super::{server_connection::ServerConnection, solver_executor::SolverResult};

#[derive(Debug, Serialize, Builder)]
pub struct SolutionUploadRequest<'a> {
    instance_id: u32,
    run_uuid: uuid::Uuid,

    #[serde(skip_serializing_if = "Option::is_none")]
    solver_uuid: Option<uuid::Uuid>,

    #[builder(setter(into, strip_option), default)]
    seconds_computed: Option<f64>,

    result: &'a SolverResult,

    #[serde(skip_serializing_if = "is_false")]
    #[builder(setter(skip))]
    dry_run: bool, // this is private and only available for testing
}

fn is_false(b: &bool) -> bool {
    !*b
}

pub fn is_score_good_enough_for_upload(solution_score: u32, best_score: Option<u32>) -> bool {
    if let Some(best_score) = best_score {
        let larger_than_score = solution_score as isize - best_score as isize;
        (larger_than_score - 5) * 10 < best_score as isize
    } else {
        true
    }
}

impl SolutionUploadRequest<'_> {
    pub async fn upload(&self, server_conn: &ServerConnection) -> anyhow::Result<()> {
        let url = server_conn.base_url().join("api/solutions/new").unwrap();

        let resp = server_conn
            .client_arc()
            .post(url)
            .json(self)
            .send()
            .await
            .expect("Failed to upload solution");

        if !resp.status().is_success() {
            debug!(
                "Failed to upload solution for IID {}; response: {:?}",
                self.instance_id,
                resp.text().await
            );
            trace!("Request was: {:?}", self);
            anyhow::bail!("Failed to upload solution");
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tracing_test::traced_test;

    use super::*;

    const IID: u32 = 549;
    const SOLUTION: [u32; 2] = [19, 70];

    #[tokio::test]
    #[traced_test]
    async fn upload_solution_minimal() {
        let solution = SolverResult::Valid {
            data: SOLUTION.into(),
        };
        let conn = ServerConnection::try_default().unwrap();

        let mut request = SolutionUploadRequestBuilder::default()
            .instance_id(IID)
            .run_uuid(uuid::Uuid::new_v4())
            .solver_uuid(None)
            .result(&solution)
            .build()
            .unwrap();

        request.dry_run = true;

        let resp = request.upload(&conn).await;
        assert!(resp.is_ok(), "{:?}", resp);
    }

    #[tokio::test]
    async fn upload_solution_with_time() {
        let solution = SolverResult::Valid {
            data: SOLUTION.into(),
        };
        let conn = ServerConnection::try_default().unwrap();

        let mut request = SolutionUploadRequestBuilder::default()
            .instance_id(IID)
            .run_uuid(uuid::Uuid::new_v4())
            .solver_uuid(None)
            .result(&solution)
            .seconds_computed(1234.0)
            .build()
            .unwrap();

        request.dry_run = true;

        request.upload(&conn).await.unwrap();
    }

    #[tokio::test]
    async fn upload_solution_with_time_and_solver() {
        let solution = SolverResult::Valid {
            data: SOLUTION.into(),
        };
        let conn = ServerConnection::try_default().unwrap();

        let mut request = SolutionUploadRequestBuilder::default()
            .instance_id(IID)
            .run_uuid(uuid::Uuid::new_v4())
            .solver_uuid(Some(uuid::Uuid::new_v4()))
            .result(&solution)
            .seconds_computed(1234.0)
            .build()
            .unwrap();

        request.dry_run = true;

        request.upload(&conn).await.unwrap();
    }
}
