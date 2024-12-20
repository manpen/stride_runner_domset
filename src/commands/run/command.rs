use std::{sync::Arc, time::Duration};
use tokio::{task, time::Instant};

use crate::{
    commands::{
        arguments::{CommonOpts, RunOpts},
        run::{
            context::RunContext,
            display::{ProgressDisplay, RunnerProgressBar},
            job::{Job, JobResult, JobResultState},
        },
    },
    utils::run_summary_logger::RunSummaryLogger,
};

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

    let avail_slots = cmd_opts.parallel_jobs;
    assert!(avail_slots > 0);
    let mut running_jobs: Vec<JobContext> = Vec::with_capacity(avail_slots);
    let mut instances = context.instance_list();

    let mut display = ProgressDisplay::new(context.clone())?;
    let mut report_error_on_exit = false;

    let mut summary_logger =
        RunSummaryLogger::try_new(&context.log_dir().join("summary.csv")).await?;

    while !(instances.is_empty() && running_jobs.is_empty()) {
        // attempt to spawn new tasks if there are available slots
        if avail_slots > running_jobs.len() {
            if let Some((iid, rest)) = instances.split_first() {
                instances = rest;
                running_jobs.push(JobContext::new(context.clone(), *iid));
            }
        }

        // poll all running tasks to see if they are finished
        // need for-loop rather than `running_jobs.drain(..)` as poll is fallible async fn
        for job_context in running_jobs.iter_mut() {
            let success = job_context.poll(&mut display, &mut summary_logger).await?;
            report_error_on_exit |= success == JobSuccess::ReportAsFailure;
        }

        // remove finished tasks from list
        running_jobs.retain_mut(|job| !job.is_finished());

        display.tick(running_jobs.len());
        let wait_for = if avail_slots > running_jobs.len() {
            SHORT_WAIT_TIME
        } else {
            DEFAULT_WAIT_TIME
        };

        tokio::time::sleep(wait_for).await;
    }

    display.final_message();
    if report_error_on_exit {
        anyhow::bail!("Some runs failed");
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobSuccess {
    ReportAsFailure,
    ReportAsSuccess,
}

struct JobContext {
    run: Arc<RunContext>,
    job: Arc<Job>,
    task_handle: Option<tokio::task::JoinHandle<Result<JobResult, anyhow::Error>>>,
    progress_bar: RunnerProgressBar,
    is_finished: bool,
}

impl JobContext {
    fn new(run: Arc<RunContext>, iid: u32) -> Self {
        let job = Arc::new(Job::new(run.clone(), iid));

        let task_handle = {
            let job_task = job.clone();
            tokio::spawn(async move { job_task.main().await })
        };

        let progress_bar = RunnerProgressBar::new(run.clone(), iid);

        Self {
            run,
            job,
            task_handle: Some(task_handle),
            progress_bar,
            is_finished: false,
        }
    }

    async fn poll(
        &mut self,
        display: &mut ProgressDisplay,
        run_logger: &mut RunSummaryLogger,
    ) -> anyhow::Result<JobSuccess> {
        while !self.task_handle.as_ref().unwrap().is_finished() {
            self.progress_bar
                .update_progress_bar(display, &self.job, Instant::now());

            task::yield_now().await;
        }

        let result = self.task_handle.take().unwrap().await??;

        let report_error_on_exit = match result.state {
            JobResultState::Optimal { .. } => JobSuccess::ReportAsSuccess, // found solution
            JobResultState::Incomplete => JobSuccess::ReportAsSuccess, // good kind of lack of success
            JobResultState::Timeout => JobSuccess::ReportAsSuccess, // good kind of lack of success
            JobResultState::Suboptimal { .. } if !self.run.cmd_opts().suboptimal_is_error => {
                JobSuccess::ReportAsSuccess
            }
            _ => JobSuccess::ReportAsFailure,
        };

        run_logger.log_job_result(self.job.iid(), &result).await?;

        self.progress_bar.finish(display, result.state);
        self.is_finished = true;

        Ok(report_error_on_exit)
    }

    fn is_finished(&self) -> bool {
        self.task_handle.is_none()
    }
}
