use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use serde::Serialize;
use std::time::Duration;
use tracing::{debug, trace};

use crate::utils::{
    instance_data_db::{DId, IId},
    solver_executor::{SolverExecutorBuilder, SolverResult},
};

use super::context::{MetaPool, RunContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerResult {
    Optimal,
    Suboptimal,
    Infeasible,
    Incomplete,
    Error,
    Timeout,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum RunnerState {
    Idle = 0,
    Fetching = 1,
    Starting = 2,
    Running = 3,
    PostProcessing = 4,
    Finished = 5,
}

struct AtomicRunnerState {
    state: AtomicU8,
}

impl AtomicRunnerState {
    fn new(state: RunnerState) -> Self {
        Self {
            state: AtomicU8::new(state as u8),
        }
    }

    fn load(&self, order: Ordering) -> RunnerState {
        match self.state.load(order) {
            x if x == RunnerState::Idle as u8 => RunnerState::Idle,
            x if x == RunnerState::Fetching as u8 => RunnerState::Fetching,
            x if x == RunnerState::Starting as u8 => RunnerState::Starting,
            x if x == RunnerState::Running as u8 => RunnerState::Running,
            x if x == RunnerState::PostProcessing as u8 => RunnerState::PostProcessing,
            x if x == RunnerState::Finished as u8 => RunnerState::Finished,
            _ => unreachable!(),
        }
    }

    fn store(&self, state: RunnerState, order: Ordering) {
        self.state.store(state as u8, order);
    }
}

pub struct Runner {
    context: Arc<RunContext>,
    iid: u32,
    state: AtomicRunnerState,
}

#[derive(Default, Debug, sqlx::FromRow)]
#[allow(non_snake_case)]
struct InstanceModel {
    iid: i32,
    data_did: u32,
    nodes: u32,
    edges: u32,
    best_score: Option<u32>,

    diameter: Option<u32>,
    treewidth: Option<u32>,
    planar: Option<bool>,
    bipartite: Option<bool>,
}

impl InstanceModel {
    fn to_env(&self) -> Vec<(String, String)> {
        let mut env = Vec::with_capacity(10);

        macro_rules! push {
            ($name:ident) => {
                env.push((
                    String::from(concat!("STRIDE_", stringify!($name))).to_ascii_uppercase(),
                    self.$name.to_string(),
                ));
            };
            (opt, $name:ident) => {
                if let Some(x) = self.$name {
                    env.push((
                        String::from(concat!("STRIDE_", stringify!($name))).to_ascii_uppercase(),
                        x.to_string(),
                    ));
                }
            };
        }

        push!(iid);
        push!(nodes);
        push!(edges);

        push!(opt, best_score);
        push!(opt, diameter);
        push!(opt, treewidth);
        push!(opt, planar);
        push!(opt, bipartite);

        env
    }
}

impl Runner {
    pub fn new(context: Arc<RunContext>, iid: u32) -> Self {
        Self {
            context,
            iid,
            state: AtomicRunnerState::new(RunnerState::Idle),
        }
    }

    pub async fn main(&self) -> anyhow::Result<RunnerResult> {
        self.update_state(RunnerState::Fetching);
        let meta = self.fetch_instance_meta_data().await?;
        let data = self
            .context
            .instance_data_db()
            .fetch_data_with_did(
                self.context.server_conn(),
                IId(self.iid),
                DId(meta.data_did),
            )
            .await?;

        self.update_state(RunnerState::Starting);
        let workdir = self.prepare_logdir()?;
        let env = self.prepare_env_variables(&meta);

        // TODO: allow passing of solver arguments
        let mut executor = SolverExecutorBuilder::default()
            .solver_path(self.context.cmd_opts().solver_binary.clone())
            .working_dir(workdir)
            .args(Vec::new())
            .timeout(self.context.cmd_opts().timeout_duration())
            .grace(self.context.cmd_opts().grace_duration())
            .instance_id(self.iid)
            .instance_data(data)
            .env(env)
            .build()
            .unwrap();

        self.update_state(RunnerState::Running);
        let result = executor.run().await?;

        self.update_state(RunnerState::PostProcessing);

        self.upload_results(&result, meta.best_score, executor.runtime().unwrap())
            .await?;
        let result = self.to_result_type(&result, &meta);

        if !self.context.cmd_opts().keep_logs_on_success {
            let successful = result == RunnerResult::Optimal
                || (result == RunnerResult::Suboptimal
                    && !self.context.cmd_opts().suboptimal_is_error);

            if successful {
                executor.delete_files()?;
            }
        }

        self.update_state(RunnerState::Finished);

        Ok(result)
    }

    pub fn state(&self) -> RunnerState {
        self.state.load(Ordering::Acquire)
    }

    #[allow(dead_code)]
    pub fn iid(&self) -> u32 {
        self.iid
    }

    fn update_state(&self, state: RunnerState) {
        trace!("Runner {} switched into state: {:?}", self.iid, state);
        self.state.store(state, Ordering::Release);
    }

    fn prepare_logdir(&self) -> anyhow::Result<std::path::PathBuf> {
        let log_base = &self.context.common_opts().run_log_dir;
        let timestamp = self.context.start().format("%y%m%d_%H%M%S");
        let dirname = format!("{}_{}", timestamp, self.context.run_uuid());

        let path = log_base.join(dirname);
        let _ = std::fs::create_dir_all(&path);

        Ok(path)
    }

    fn prepare_env_variables(&self, meta: &InstanceModel) -> Vec<(String, String)> {
        if self.context.cmd_opts().no_env {
            return Vec::new();
        }

        let mut env = meta.to_env();
        env.push((
            "STRIDE_TIMEOUT_SEC".into(),
            self.context.cmd_opts().timeout.to_string(),
        ));
        env.push((
            "STRIDE_GRACE_SEC".into(),
            self.context.cmd_opts().grace.to_string(),
        ));
        env.push((
            "STRIDE_RUN_UUID".to_string(),
            self.context.run_uuid().to_string(),
        ));
        if let Some(x) = self.context.cmd_opts().solver_uuid.as_ref() {
            env.push(("STRIDE_SOLVER_UUID".to_string(), x.to_string()));
        }
        env
    }

    async fn fetch_instance_meta_data(&self) -> anyhow::Result<InstanceModel> {
        let MetaPool(pool) = self.context.db_meta();
        let instance = sqlx::query_as::<_, InstanceModel>(
            r#"
                SELECT iid, data_did, nodes, edges, best_score, diameter, treewidth, planar, bipartite
                FROM Instance
                WHERE iid = ?
            "#,
        )
        .bind(self.iid)
        .fetch_one(pool)
        .await?;

        Ok(instance)
    }

    async fn upload_results(
        &self,
        result: &SolverResult,
        best_score: Option<u32>,
        runtime: Duration,
    ) -> anyhow::Result<()> {
        if self.context.cmd_opts().no_upload {
            return Ok(());
        }

        if self.context.cmd_opts().solver_uuid.is_none() {
            let nice_result = match result {
                SolverResult::Valid { data } => {
                    if let Some(best_score) = best_score {
                        let larger_than_score = data.len() as isize - best_score as isize;
                        (larger_than_score - 5) * 10 < best_score as isize
                    } else {
                        true
                    }
                }

                _ => false,
            };

            if !nice_result {
                return Ok(());
            }
        }

        #[derive(Debug, Serialize)]
        struct SolutionUploadRequest<'a> {
            instance_id: u32,
            run_uuid: uuid::Uuid,
            solver_uuid: Option<uuid::Uuid>,
            seconds_computed: f64,
            result: &'a SolverResult,
        }

        let request = SolutionUploadRequest {
            instance_id: self.iid,
            run_uuid: self.context.run_uuid(),
            solver_uuid: self.context.cmd_opts().solver_uuid,
            seconds_computed: runtime.as_secs_f64(),
            result,
        };

        let url = self
            .context
            .server_conn()
            .base_url()
            .join("api/solutions/new")
            .unwrap();
        let client = self.context.server_conn().client_arc();

        let resp = client
            .post(url)
            .json(&request)
            .send()
            .await
            .expect("Failed to upload solution");

        if !resp.status().is_success() {
            debug!(
                "Failed to upload solution for IID {}; response: {:?}",
                self.iid,
                resp.text().await
            );
            trace!("Request was: {:?}", request);
            anyhow::bail!("Failed to upload solution");
        }

        Ok(())
    }

    fn to_result_type(&self, result: &SolverResult, meta: &InstanceModel) -> RunnerResult {
        match &result {
            // at this point, we have a valid solution
            SolverResult::Valid { data } => {
                let larger_than_best = meta
                    .best_score
                    .map_or(0, |x| data.len() as isize - x as isize);

                if larger_than_best <= 0 {
                    RunnerResult::Optimal
                } else {
                    RunnerResult::Suboptimal
                }
            }
            SolverResult::ValidCached => unreachable!(),
            SolverResult::Infeasible => RunnerResult::Infeasible,
            SolverResult::SyntaxError => RunnerResult::Error,
            SolverResult::Timeout => RunnerResult::Timeout,
            SolverResult::IncompleteOutput => RunnerResult::Incomplete,
        }
    }
}
