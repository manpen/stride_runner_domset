use std::sync::Arc;

use console::{Attribute, Style};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::time::Instant;

use super::{
    context::RunContext,
    job::{Job, JobResultState, JobState},
};

pub struct ProgressDisplay {
    context: Arc<RunContext>,
    mpb: MultiProgress,
    link_line: ProgressBar,
    status_line: ProgressBar,
    pb_total: ProgressBar,

    num_optimal: u64,
    num_suboptimal: u64,
    num_infeasible: u64,
    num_error: u64,
    num_timeout: u64,
    num_incomplete: u64,
}

impl ProgressDisplay {
    pub fn new(context: Arc<RunContext>) -> anyhow::Result<Self> {
        let mpb = MultiProgress::new();

        let link_line = mpb.add(ProgressBar::no_length());
        link_line.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());
        link_line.set_message(
            match (
                &context.cmd_opts().solver_uuid,
                context.cmd_opts().no_upload,
            ) {
                (_, true) => {
                    format!(
                        "{} | Run: {}",
                        Style::new().red().apply_to("upload disabled"),
                        context.run_uuid()
                    )
                }
                (Some(uuid), false) => {
                    let url = context
                        .server_conn()
                        .solver_website_for_user(*uuid)
                        .to_string();

                    format!("visit {url} | Run: {}", context.run_uuid())
                }
                (_, false) => {
                    format!(
                        "{} | Run: {}",
                        Style::new()
                            .yellow()
                            .apply_to("consider to register solver for more stats"),
                        context.run_uuid()
                    )
                }
            },
        );

        let status_line = mpb.add(ProgressBar::no_length());
        status_line.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());

        let pb_total = mpb.add(indicatif::ProgressBar::new(
            context.instance_list().len() as u64
        ));
        pb_total.set_style(
            ProgressStyle::default_bar()
                .template("{msg:<15} [{elapsed_precise}] [{bar:50.green/grey}] {human_pos} of {human_len} (est: {eta})")?
                .progress_chars("#>-"),
        );

        pb_total.set_message("Total finished");

        Ok(Self {
            context,
            mpb,
            link_line,
            status_line,
            pb_total,
            num_optimal: 0,
            num_suboptimal: 0,
            num_infeasible: 0,
            num_error: 0,
            num_timeout: 0,
            num_incomplete: 0,
        })
    }

    fn multi_progress(&self) -> &MultiProgress {
        &self.mpb
    }

    pub fn tick(&mut self, running: usize) {
        macro_rules! format_num {
            ($key:ident, $name:expr, $color:ident) => {
                format_num!($key, $name, $color, [])
            };
            ($key:ident, $name:expr, $color:ident, $attrs : expr) => {{
                let text = format!("{}: {:>6}", $name, self.$key);
                if self.$key == 0 {
                    text
                } else {
                    let mut style = console::Style::new().$color();
                    for x in $attrs {
                        style = style.attr(x);
                    }

                    style.apply_to(text).to_string()
                }
            }};
        }

        const CRITICAL: [Attribute; 2] = [Attribute::Bold, Attribute::Underlined];
        let parts = [
            format_num!(num_optimal, "Opt", green),
            if self.context.cmd_opts().suboptimal_is_error {
                format_num!(num_suboptimal, "Subopt", red, CRITICAL)
            } else {
                format_num!(num_suboptimal, "Subopt", blue)
            },
            format_num!(num_incomplete, "Incomp", yellow),
            format_num!(num_timeout, "Timeout", yellow),
            format_num!(num_error, "Err", red),
            format_num!(num_infeasible, "Infeas", red, CRITICAL),
            format!("Running: {}", running),
        ];

        self.status_line.set_message(parts.join(" | "));
    }

    pub fn finish_job(&mut self, _iid: u32, status: JobResultState) {
        self.pb_total.inc(1);

        match status {
            JobResultState::BestKnown { .. } => self.num_optimal += 1,
            JobResultState::Suboptimal { .. } => self.num_suboptimal += 1,
            JobResultState::Infeasible => self.num_infeasible += 1,
            JobResultState::Error => self.num_error += 1,
            JobResultState::Timeout => self.num_timeout += 1,
            JobResultState::Incomplete => self.num_incomplete += 1,
        }
    }

    pub fn final_message(&self) {
        println!("{}", self.link_line.message());
        println!("{}", self.status_line.message());
    }
}

pub struct RunnerProgressBar {
    context: Arc<RunContext>,
    iid: u32,
    pb: Option<ProgressBar>,
    previous_state: Option<JobState>,
    start: tokio::time::Instant,
    max_time_millis: u64,
}

impl RunnerProgressBar {
    const MILLIS_BEFORE_PROGRESS_BAR: u64 = 100;

    pub fn new(context: Arc<RunContext>, iid: u32) -> Self {
        let max_time_millis = (context.cmd_opts().timeout + context.cmd_opts().grace) * 1000;
        Self {
            context,
            iid,
            start: tokio::time::Instant::now(),
            max_time_millis,
            pb: None,
            previous_state: None,
        }
    }

    pub fn update_progress_bar(&mut self, mpb: &ProgressDisplay, runner: &Job, now: Instant) {
        let elapsed = (now.duration_since(self.start).as_millis() as u64).min(self.max_time_millis);
        if elapsed < Self::MILLIS_BEFORE_PROGRESS_BAR {
            return; // do not create a progress bar for short running tasks
        }

        if self.pb.is_none() {
            self.create_pb(mpb.multi_progress());
        }

        let pb = self.pb.as_ref().unwrap();

        let runner_state = runner.state();
        if Some(runner_state) != self.previous_state {
            self.previous_state = Some(runner_state);
            self.start = now;
            self.pb.as_ref().unwrap().reset_elapsed();

            if runner_state == JobState::Running {
                self.style_for_running(pb);
            } else {
                self.style_for_waiting(pb);
            }
        }

        let message: String = match runner.state() {
            JobState::Idle => "startup".into(),
            JobState::Fetching => "fetching data".into(),
            JobState::Starting => "starting".into(),
            JobState::Running => {
                if 1 > self.context.cmd_opts().timeout * 1000 {
                    Style::new().red().apply_to("grace").to_string()
                } else {
                    "running".into()
                }
            }
            JobState::PostProcessing => "post-processing / upload".into(),
            JobState::Finished => "done".into(),
        };

        pb.set_message(message);
        pb.set_position(elapsed);
    }

    pub fn finish(&self, display: &mut ProgressDisplay, status: JobResultState) {
        if let Some(pb) = &self.pb {
            display.multi_progress().remove(pb);
        }

        display.finish_job(self.iid, status);
    }

    fn create_pb(&mut self, mpb: &MultiProgress) {
        let pb = mpb.add(ProgressBar::new(self.max_time_millis));
        self.pb = Some(pb);
    }

    fn style_for_running(&self, pb: &ProgressBar) {
        let mut template = format!("Inst. ID {: >6} ", self.iid);
        template += "[{elapsed_precise}] [{bar:50.cyan/blue}] {msg}";

        pb.set_style(
            ProgressStyle::default_bar()
                .template(&template)
                .unwrap()
                .progress_chars("#>-"),
        );

        pb.set_length(self.max_time_millis);
    }

    fn style_for_waiting(&self, pb: &ProgressBar) {
        let mut template = format!("Inst. ID {: >6} ", self.iid);
        template += "[{elapsed_precise}] {spinner:.green}                                                    {msg}";

        pb.set_style(ProgressStyle::default_bar().template(&template).unwrap());
    }
}
