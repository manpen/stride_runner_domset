use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

use crate::commands::{
    arguments::{CommonOpts, RunOpts},
    run::{
        context::RunContext,
        display::{ProgressDisplay, RunnerProgressBar},
        job::{Job, TaskResult, JobResultState},
    },
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
    let mut running_jobs = Vec::with_capacity(avail_slots);

    let mut display = ProgressDisplay::new(context.clone())?;

    let mut next_instace = 0;

    let mut report_error_on_exit = false;

    while next_instace < context.instance_list().len() || !running_jobs.is_empty() {
        if avail_slots > running_jobs.len() && next_instace < context.instance_list().len() {
            let iid = context.instance_list()[next_instace];
            next_instace += 1;

            let job = Arc::new(Job::new(context.clone(), iid));
            let job_task_handle: tokio::task::JoinHandle<Result<TaskResult, anyhow::Error>> = {
                let job_task = job.clone();
                tokio::spawn(async move { job_task.main().await })
            };

            running_jobs.push((
                job_task_handle,
                job,
                RunnerProgressBar::new(context.clone(), iid),
                true,
            ));
        }

        let now = Instant::now();
        for (handle, runner, progress_bar, keep) in running_jobs.iter_mut() {
            if handle.is_finished() {
                let main_result = handle.await??;

                report_error_on_exit |= match main_result.state {
                    JobResultState::Optimal { .. } => false, // found solution
                    JobResultState::Incomplete => false,     // good kind of lack of success
                    JobResultState::Timeout => false,        // good kind of lack of success
                    JobResultState::Suboptimal { .. } => cmd_opts.suboptimal_is_error,
                    _ => true,
                };

                progress_bar.finish(&mut display, main_result.state);
                *keep = false;
            } else {
                progress_bar.update_progress_bar(&display, runner, now);
            }
        }

        running_jobs.retain_mut(|x| x.3);

        display.tick(running_jobs.len());

        tokio::time::sleep(if avail_slots > running_jobs.len() {
            SHORT_WAIT_TIME
        } else {
            DEFAULT_WAIT_TIME
        })
        .await;
    }

    display.final_message();

    if report_error_on_exit {
        anyhow::bail!("Some runs failed");
    }

    Ok(())
}
