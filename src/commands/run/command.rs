use std::{path::PathBuf, sync::Arc, time::Duration};

use structopt::StructOpt;
use tokio::time::Instant;
use uuid::Uuid;

use crate::commands::{
    common::CommonOpts,
    run::{
        context::RunContext,
        display::{ProgressDisplay, RunnerProgressBar},
        runner::{Runner, RunnerResult},
    },
};

#[derive(Clone, Debug, StructOpt)]
pub struct RunOpts {
    pub solver_binary: PathBuf,

    #[structopt(
        short = "-S",
        long,
        help = "UUID of the solver to be used; enables upload of all results for later analysis"
    )]
    pub solver_uuid: Option<Uuid>,

    #[structopt(
        short = "-T",
        long,
        help = "Send SIGTERM after that many seconds",
        default_value = "300"
    )]
    pub timeout: u64,

    #[structopt(
        short = "-G",
        long,
        help = "Kill solver after that many seconds after SIGTERM",
        default_value = "5"
    )]
    pub grace: u64,

    #[structopt(short = "-j", long, help = "Max. number of parallel solver runs")]
    pub parallel_jobs: Option<usize>,

    #[structopt(
        short = "-o",
        long,
        help = "Set for exact solvers; treats sub-optimal solutions as errors."
    )]
    pub suboptimal_is_error: bool,

    #[structopt(long, help = "Sort instance list by IID; otherwise shuffle")]
    pub sort_instances: bool,

    #[structopt(
        short = "-i",
        long,
        help = "Path to a file with instance list (one IID per line) to be used as input"
    )]
    pub instances: Option<PathBuf>,

    #[structopt(
        short = "-w",
        long = "--where",
        help = "SELECT iid FROM Instance WHERE ...; if combined with -i the intersection is taken"
    )]
    pub sql_where: Option<String>,

    #[structopt(short = "-e", help = "Export instances to a file")]
    pub export_iid_only: Option<PathBuf>,

    #[structopt(
        short = "-n",
        help = "Upload nothing, not even good solutions. PLEASE DO NOT USE SINCE THIS IS A COMMUNITY TOOL"
    )]
    pub upload_nothing: bool,

    #[structopt(
        short = "-E",
        help = "Do not set environment variables (STRIDE_*) for solver"
    )]
    pub no_env: bool,

    #[structopt(
        short = "-k",
        long,
        help = "Keep logs of successful runs in `stride-logs` dir (default: only failed runs)"
    )]
    pub keep_logs_on_success: bool,
}

impl RunOpts {
    pub fn timeout_duration(&self) -> Duration {
        Duration::from_secs(self.timeout)
    }

    pub fn grace_duration(&self) -> Duration {
        Duration::from_secs(self.grace)
    }
}

const DEFAULT_WAIT_TIME: Duration = Duration::from_millis(100);
const SHORT_WAIT_TIME: Duration = Duration::from_millis(10);

pub async fn command_run(common_opts: &CommonOpts, cmd_opts: &RunOpts) -> anyhow::Result<()> {
    if !cmd_opts.solver_binary.is_file() {
        anyhow::bail!("Solver binary {:?} not found", cmd_opts.solver_binary);
    }

    let context = Arc::new({
        // we begin with an exclusive hold on the context; after leaving this block, we may not modify it
        let mut context = RunContext::new(common_opts.clone(), cmd_opts.clone()).await?;
        context.build_instance_list().await?;

        if context.instance_list().is_empty() {
            anyhow::bail!("No instances to run");
        }
        if let Some(path) = cmd_opts.export_iid_only.as_ref() {
            context.write_instance_list(path)?;
            println!("Wrote instance list to {:?}. Done", path);
            return Ok(());
        }

        context
    });

    let avail_slots = cmd_opts.parallel_jobs.unwrap_or(num_cpus::get());
    assert!(avail_slots > 0);
    let mut running_tasks = Vec::with_capacity(avail_slots);

    let mut display = ProgressDisplay::new(context.clone())?;

    let mut next_instace = 0;

    while next_instace < context.instance_list().len() || !running_tasks.is_empty() {
        if avail_slots > running_tasks.len() && next_instace < context.instance_list().len() {
            let iid = context.instance_list()[next_instace];
            next_instace += 1;

            let runner = Arc::new(Runner::new(context.clone(), iid));
            let handle: tokio::task::JoinHandle<Result<RunnerResult, anyhow::Error>> = {
                let runner = runner.clone();
                tokio::spawn(async move { runner.main().await })
            };
            running_tasks.push((
                handle,
                runner,
                RunnerProgressBar::new(context.clone(), iid),
                true,
            ));
        }

        let now = Instant::now();
        for (handle, runner, progress_bar, keep) in running_tasks.iter_mut() {
            if handle.is_finished() {
                let result = handle.await??;
                progress_bar.finish(&mut display, result);
                *keep = false;
            } else {
                progress_bar.update_progress_bar(&display, runner, now);
            }
        }

        running_tasks.retain_mut(|x| x.3);

        display.tick(running_tasks.len());

        tokio::time::sleep(if avail_slots > running_tasks.len() {
            SHORT_WAIT_TIME
        } else {
            DEFAULT_WAIT_TIME
        })
        .await;
    }

    Ok(())
}
