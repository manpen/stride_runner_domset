use std::{path::PathBuf, sync::Arc, time::Duration};

use structopt::StructOpt;
use tokio::time::Instant;
use tracing::info;
use uuid::Uuid;

use crate::commands::{
    common::CommonOpts,
    run::{
        context::RunContext,
        display::{ProgressDisplay, RunnerProgressBar},
        runner::Runner,
    },
};

#[derive(Clone, Debug, StructOpt)]
pub struct RunOpts {
    pub solver_binary: PathBuf,

    #[structopt(short = "-S", long)]
    pub solver_uuid: Option<Uuid>,

    #[structopt(short = "-T", long, default_value = "300")]
    pub timeout: u64,

    #[structopt(short = "-G", long, default_value = "5")]
    pub grace: u64,

    #[structopt(short = "-j", long)]
    pub parallel_jobs: Option<usize>,

    #[structopt(short = "-o", long)]
    pub report_non_optimal: bool,

    #[structopt(long, help = "Sort instance list by IID; otherwise shuffle")]
    pub sort_instances: bool,

    #[structopt(short = "-i", long)]
    pub instances: Option<PathBuf>,

    #[structopt(short = "-w", long = "--where", help = "SQL WHERE clause")]
    pub sql_where: Option<String>,

    #[structopt(short = "-e", help = "Export instances to a file")]
    pub export_iid_only: Option<PathBuf>,

    #[structopt(
        short = "-u",
        help = "Upload all results to be viewed over the web interface"
    )]
    pub upload_all: bool,

    #[structopt(
        short = "-n",
        help = "Upload nothing, not even good solutions. PLEASE DO NOT USE"
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
            let handle: tokio::task::JoinHandle<Result<(), anyhow::Error>> = {
                let runner = runner.clone();
                tokio::spawn(async move { runner.main().await })
            };
            running_tasks.push((
                Some(handle),
                runner,
                RunnerProgressBar::new(context.clone(), iid),
            ));
        }

        // see whether any runner finished with an error
        for (handle, runner, _) in running_tasks.iter_mut() {
            if !handle.as_ref().is_some_and(|x| x.is_finished()) {
                continue;
            }

            let handle = handle.take();
            if let Some(handle) = handle {
                if let Err(e) = handle.await {
                    info!("Runner of IID {} failed with: {:?}", runner.iid(), e);
                    return Err(e.into());
                }
            }
        }

        let now = Instant::now();
        running_tasks.retain_mut(|(_, runner, progress_bar)| {
            if let Some(status) = runner.try_take_result() {
                progress_bar.finish(&mut display, status);
                false
            } else {
                progress_bar.update_progress_bar(&display, runner, now);
                true
            }
        });

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
