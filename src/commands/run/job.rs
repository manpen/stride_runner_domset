use std::{
    fmt::Display,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use std::time::Duration;
use tracing::trace;

use crate::utils::{
    instance_data_db::{DId, IId},
    solution_upload::{is_score_good_enough_for_upload, SolutionUploadRequestBuilder},
    solver_executor::{SolverExecutorBuilder, SolverResult},
};

use super::context::{MetaPool, RunContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobResultState {
    Optimal { score: u32 },
    Suboptimal { score: u32, best_known: u32 },
    Infeasible,
    Incomplete,
    Error,
    Timeout,
}

impl Display for JobResultState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Optimal { .. } => "optimal",
            Self::Suboptimal { .. } => "suboptimal",
            Self::Infeasible => "infeasible",
            Self::Incomplete => "incomplete",
            Self::Error => "error",
            Self::Timeout => "timeout",
        })
    }
}

impl JobResultState {
    pub fn is_optimal(&self) -> bool {
        matches!(self, Self::Optimal { .. })
    }

    pub fn is_suboptimal(&self) -> bool {
        matches!(self, Self::Suboptimal { .. })
    }
}

pub struct JobResult {
    pub state: JobResultState,
    pub runtime: Duration,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum JobState {
    Idle = 0,
    Fetching = 1,
    Starting = 2,
    Running = 3,
    PostProcessing = 4,
    Finished = 5,
}

struct AtomicJobState {
    state: AtomicU8,
}

impl AtomicJobState {
    fn new(state: JobState) -> Self {
        Self {
            state: AtomicU8::new(state as u8),
        }
    }

    fn load(&self, order: Ordering) -> JobState {
        match self.state.load(order) {
            x if x == JobState::Idle as u8 => JobState::Idle,
            x if x == JobState::Fetching as u8 => JobState::Fetching,
            x if x == JobState::Starting as u8 => JobState::Starting,
            x if x == JobState::Running as u8 => JobState::Running,
            x if x == JobState::PostProcessing as u8 => JobState::PostProcessing,
            x if x == JobState::Finished as u8 => JobState::Finished,
            _ => unreachable!(),
        }
    }

    fn store(&self, state: JobState, order: Ordering) {
        self.state.store(state as u8, order);
    }
}

pub struct Job {
    context: Arc<RunContext>,
    iid: u32,
    state: AtomicJobState,
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

impl Job {
    pub fn new(context: Arc<RunContext>, iid: u32) -> Self {
        Self {
            context,
            iid,
            state: AtomicJobState::new(JobState::Idle),
        }
    }

    pub async fn main(&self) -> anyhow::Result<JobResult> {
        self.update_state(JobState::Fetching);
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

        self.update_state(JobState::Starting);
        let env = self.prepare_env_variables(&meta);

        let mut executor = SolverExecutorBuilder::default()
            .solver_path(self.context.cmd_opts().solver_binary.clone())
            .working_dir(self.context.log_dir().to_path_buf())
            .args(self.context.cmd_opts().solver_args.clone())
            .timeout(self.context.cmd_opts().timeout_duration())
            .grace(self.context.cmd_opts().grace_duration())
            .instance_id(self.iid)
            .instance_data(data)
            .env(env)
            .build()
            .unwrap();

        self.update_state(JobState::Running);
        let result = executor.run().await?;

        self.update_state(JobState::PostProcessing);

        let runtime = executor.runtime().unwrap();

        self.upload_results(&result, meta.best_score, runtime)
            .await?;
        let result = self.to_result_type(&result, &meta);

        if !self.context.cmd_opts().keep_logs_on_success {
            let successful = result.is_optimal()
                || (result.is_suboptimal() && !self.context.cmd_opts().suboptimal_is_error);

            if successful {
                executor.delete_files()?;
            }
        }

        self.update_state(JobState::Finished);

        Ok(JobResult {
            state: result,
            runtime,
        })
    }

    pub fn state(&self) -> JobState {
        self.state.load(Ordering::Acquire)
    }

    #[allow(dead_code)]
    pub fn iid(&self) -> u32 {
        self.iid
    }

    fn update_state(&self, state: JobState) {
        trace!("Runner {} switched into state: {:?}", self.iid, state);
        self.state.store(state, Ordering::Release);
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
                    is_score_good_enough_for_upload(data.len() as u32, best_score)
                }

                _ => false,
            };

            if !nice_result {
                return Ok(());
            }
        }

        let request = SolutionUploadRequestBuilder::default()
            .instance_id(self.iid)
            .run_uuid(self.context.run_uuid())
            .solver_uuid(self.context.cmd_opts().solver_uuid)
            .seconds_computed(runtime.as_secs_f64())
            .result(result)
            .build()
            .unwrap();

        request.upload(self.context.server_conn()).await?;

        Ok(())
    }

    fn to_result_type(&self, result: &SolverResult, meta: &InstanceModel) -> JobResultState {
        match &result {
            // at this point, we have a valid solution
            SolverResult::Valid { data } => {
                let larger_than_best = meta
                    .best_score
                    .map_or(0, |x| data.len() as isize - x as isize);

                if larger_than_best <= 0 {
                    JobResultState::Optimal {
                        score: data.len() as u32,
                    }
                } else {
                    JobResultState::Suboptimal {
                        score: data.len() as u32,
                        best_known: meta.best_score.unwrap(), // cannot fail since larger_than_best > 0
                    }
                }
            }
            SolverResult::ValidCached => unreachable!(),
            SolverResult::Infeasible => JobResultState::Infeasible,
            SolverResult::SyntaxError => JobResultState::Error,
            SolverResult::Timeout => JobResultState::Timeout,
            SolverResult::IncompleteOutput => JobResultState::Incomplete,
        }
    }
}
