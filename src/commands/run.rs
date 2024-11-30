use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::debug;
use rand::{seq::SliceRandom, Rng};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    io::BufRead,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use structopt::StructOpt;
use tokio::{sync::Mutex, time::Instant};
use uuid::{timestamp::context, Uuid};

use crate::{commands::run, utils::solver_executor::SolverResult};

use super::common::CommonOpts;

struct MetaPool(SqlitePool);
struct InstanceDataPool(SqlitePool);

#[derive(Clone, Debug, StructOpt)]
pub struct RunOpts {
    #[structopt(short = "-S", long)]
    solver_uuid: Option<Uuid>,

    #[structopt(short = "-T", long, default_value = "300")]
    timeout: u64,

    #[structopt(short = "-G", long, default_value = "5")]
    grace: u64,

    #[structopt(short = "-j", long)]
    parallel_jobs: Option<usize>,

    #[structopt(short = "-o", long)]
    report_non_optimal: bool,

    #[structopt(short = "-i", long)]
    instances: Option<PathBuf>,

    #[structopt(short = "-w", long = "--where", help = "SQL WHERE clause")]
    sql_where: Option<String>,

    #[structopt(short = "-e", help = "Export instances to a file")]
    export_iid_only: bool,

    #[structopt(
        short = "-u",
        help = "Upload all results to be viewed over the web interface"
    )]
    upload_all: bool,

    #[structopt(
        short = "-n",
        help = "Upload nothing, not even good solutions. PLEASE DO NOT USE"
    )]
    upload_nothing: bool,

    #[structopt(
        short = "-E",
        help = "Do not set environment variables (STRIDE_*) for solver"
    )]
    no_env: bool,
}

/// Reads a newline separated list of instance IDs from a file.
/// Whitespaces are trimmed from the beginning and end of each line.
/// Lines starting with 'c' are considered comments and ignored.
fn read_instance_list(path: &Path) -> anyhow::Result<Vec<u32>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let mut instances = Vec::new();
    for org_line in reader.lines() {
        let org_line = org_line?;
        let line = org_line.trim();

        if line.is_empty() || line.starts_with("c") {
            continue;
        }

        let id = line.parse::<u32>()?;
        instances.push(id);
    }

    debug!("Read {} instances from {:?}", instances.len(), path);
    Ok(instances)
}

async fn fetch_instances_from_db(
    MetaPool(db): &MetaPool,
    where_clause: &str,
) -> anyhow::Result<Vec<u32>> {
    // there might be some "security" implications here, but I do not really care:
    // the sqlite database is fully under user control and worst-case the
    // user needs to re-pull it after they (intentionally) messed it up ...
    let instances =
        sqlx::query_scalar::<_, u32>(&format!("SELECT iid FROM Instance WHERE {}", where_clause))
            .fetch_all(db)
            .await?;

    debug!(
        "Read {} instances from InstanceDB where {}",
        instances.len(),
        where_clause
    );

    Ok(instances)
}

struct RunContext {
    common_opts: CommonOpts,
    cmd_opts: RunOpts,

    db_meta: MetaPool,
    db_instances: InstanceDataPool,
    //db_cache: SqlitePool,
}

impl RunContext {
    pub async fn new(common_opts: CommonOpts, cmd_opts: RunOpts) -> anyhow::Result<Self> {
        let stride_dir = common_opts.stride_dir()?;

        Ok(Self {
            common_opts,
            cmd_opts,

            db_meta: MetaPool(Self::open_db_pool(stride_dir.db_meta_file().as_path()).await?),
            db_instances: InstanceDataPool(
                Self::open_db_pool(stride_dir.db_instance_file().as_path()).await?,
            ),
            //db_cache: Self::open_db_pool(stride_dir.db_cache_file().as_path()).await?,
        })
    }

    async fn open_db_pool(path: &Path) -> anyhow::Result<SqlitePool> {
        if !path.is_file() {
            anyhow::bail!("Database file {path:?} does not exist. Run the >update< command first");
        }

        let pool = sqlx::sqlite::SqlitePool::connect(
            format!("sqlite:{}", path.to_str().expect("valid path name")).as_str(),
        )
        .await?;
        Ok(pool)
    }

    async fn build_instance_list(&self) -> anyhow::Result<Vec<u32>> {
        if self.cmd_opts.instances.is_none() && self.cmd_opts.sql_where.is_none() {
            anyhow::bail!("Must prove --instances and/or --sql-where");
        }

        let instances_from_file = match &self.cmd_opts.instances {
            Some(path) => Some(read_instance_list(path.as_path())?),
            None => None,
        };

        let instances_from_db = match &self.cmd_opts.sql_where {
            Some(where_clause) => Some(fetch_instances_from_db(&self.db_meta, where_clause).await?),
            None => None,
        };

        let mut instance = match (instances_from_file, instances_from_db) {
            (Some(file), Some(db)) => {
                let file: HashSet<_> = file.into_iter().collect();
                let db: HashSet<_> = db.into_iter().collect();
                file.intersection(&db).cloned().collect()
            }
            (Some(file), None) => file,
            (None, Some(db)) => db,
            (None, None) => unreachable!(),
        };

        instance.shuffle(&mut rand::thread_rng());

        Ok(instance)
    }

    pub fn run(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunnerResult {
    Optimal,
    Suboptimal,
    Infeasible,
    Incomplete,
    Error,
    Timeout,
}

struct Runner {
    context: Arc<RunContext>,
    iid: u32,
    result: Mutex<Option<RunnerResult>>,
}

impl Runner {
    fn new(context: Arc<RunContext>, iid: u32) -> Self {
        Self {
            context,
            iid,
            result: Mutex::new(None),
        }
    }

    async fn main(&self) {
        let seconds = rand::thread_rng().gen_range(
            self.context.cmd_opts.timeout / 4
                ..=(self.context.cmd_opts.timeout + self.context.cmd_opts.grace),
        );
        tokio::time::sleep(Duration::from_secs(seconds)).await;

        let mut result = self.result.lock().await;

        let results = [
            RunnerResult::Optimal,
            RunnerResult::Suboptimal,
            RunnerResult::Infeasible,
            RunnerResult::Incomplete,
            RunnerResult::Error,
            RunnerResult::Timeout,
        ];

        *result = Some(*results.choose(&mut rand::thread_rng()).unwrap());
    }

    pub fn try_take_result(&self) -> Option<RunnerResult> {
        match self.result.try_lock() {
            Ok(mut x) => x.take(),
            Err(_) => None,
        }
    }
}

struct RunnerProgressBar {
    context: Arc<RunContext>,
    iid: u32,
    pb: Option<ProgressBar>,
    start: tokio::time::Instant,
    max_time_millis: u64,
}

impl RunnerProgressBar {
    const MILLIS_BEFORE_PROGRESS_BAR: u64 = 500;

    pub fn new(context: Arc<RunContext>, iid: u32) -> Self {
        let max_time_millis = (context.cmd_opts.timeout + context.cmd_opts.grace) * 1000;
        Self {
            context,
            iid,
            start: tokio::time::Instant::now(),
            max_time_millis,
            pb: None,
        }
    }

    pub fn update_progress_bar(&mut self, mpb: &ProgressDisplay, now: Instant) {
        let elapsed = (now.duration_since(self.start).as_millis() as u64).min(self.max_time_millis);
        if elapsed < Self::MILLIS_BEFORE_PROGRESS_BAR {
            return; // do not create a progress bar for short running tasks
        }

        if self.pb.is_none() {
            self.create_pb(mpb.multi_progress());
        }

        let pb = self.pb.as_ref().unwrap();
        // there exists a progess bar -- update it (otherwise we first init it)
        if elapsed > self.context.cmd_opts.timeout * 1000 {
            pb.set_style(
                indicatif::ProgressStyle::default_bar()
                    .template("{msg} [{elapsed_precise}] [{bar:50.red/blue}] SIGTERM sent; grace")
                    .unwrap()
                    .progress_chars("#>-"),
            );
        }

        pb.set_position(elapsed);
    }

    pub fn finish(&self, display: &mut ProgressDisplay, status: RunnerResult) {
        if let Some(pb) = &self.pb {
            display.multi_progress().remove(pb);
        }

        display.finish_job(self.iid, status);
    }

    fn create_pb(&mut self, mpb: &MultiProgress) {
        let pb = mpb.add(indicatif::ProgressBar::new(self.max_time_millis));
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{msg} [{elapsed_precise}] [{bar:50.cyan/blue}]")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(format!("Inst. ID {: >6}", self.iid));
        self.pb = Some(pb);
    }
}

struct ProgressDisplay {
    context: Arc<RunContext>,
    mpb: MultiProgress,
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
    fn new(context: Arc<RunContext>, num_instances: usize) -> anyhow::Result<Self> {
        let mpb = MultiProgress::new();

        let status_line = mpb.add(ProgressBar::no_length());
        status_line.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());

        let pb_total = mpb.add(indicatif::ProgressBar::new(num_instances as u64));
        pb_total.set_style(
            ProgressStyle::default_bar()
                .template("{msg:<15} [{elapsed_precise}] [{bar:50.green/grey}] {human_pos} of {human_len} (est: {eta})")?
                .progress_chars("#>-"),
        );

        pb_total.set_message("Total finished");

        Ok(Self {
            context,
            mpb,
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
        use console::{Attribute, Style};

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
            if self.context.cmd_opts.report_non_optimal {
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

    pub fn finish_job(&mut self, _iid: u32, status: RunnerResult) {
        self.pb_total.inc(1);

        match status {
            RunnerResult::Optimal => self.num_optimal += 1,
            RunnerResult::Suboptimal => self.num_suboptimal += 1,
            RunnerResult::Infeasible => self.num_infeasible += 1,
            RunnerResult::Error => self.num_error += 1,
            RunnerResult::Timeout => self.num_timeout += 1,
            RunnerResult::Incomplete => self.num_incomplete += 1,
        }
    }
}

pub async fn command_run(common_opts: &CommonOpts, cmd_opts: &RunOpts) -> anyhow::Result<()> {
    let context = Arc::new(RunContext::new(common_opts.clone(), cmd_opts.clone()).await?);

    let mut instances = context.build_instance_list().await?;

    let avail_slots = cmd_opts.parallel_jobs.unwrap_or(num_cpus::get());
    assert!(avail_slots > 0);
    let mut running_tasks = Vec::with_capacity(avail_slots);

    let mut display = ProgressDisplay::new(context.clone(), instances.len())?;

    while !instances.is_empty() || !running_tasks.is_empty() {
        if avail_slots > running_tasks.len() {
            let iid = match instances.pop() {
                Some(iid) => iid,
                None => break,
            };

            let runner = Arc::new(Runner::new(context.clone(), iid));
            running_tasks.push((runner.clone(), RunnerProgressBar::new(context.clone(), iid)));

            tokio::spawn(async move { runner.main().await });
        }

        let now = Instant::now();
        running_tasks.retain_mut(|(runner, progress_bar)| {
            if let Some(status) = runner.try_take_result() {
                progress_bar.finish(&mut display, status);
                false
            } else {
                progress_bar.update_progress_bar(&display, now);
                true
            }
        });

        display.tick(running_tasks.len());

        tokio::time::sleep(Duration::from_millis(
            if avail_slots > running_tasks.len() {
                10
            } else {
                100
            },
        ))
        .await;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::io::Write;
    use tempdir::TempDir;

    const PREFIX: &str = "run-test";

    #[test]
    fn read_instance_list() {
        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let instances_file = tmp_dir.path().join("instances.txt");

        // write some instances to the file
        {
            let mut file = std::fs::File::create(&instances_file).unwrap();
            writeln!(file, "c comment").unwrap();
            writeln!(file, " 1").unwrap();
            writeln!(file).unwrap();
            writeln!(file, "712 ").unwrap();
            writeln!(file, " 4").unwrap();
            writeln!(file, "  ").unwrap();
            writeln!(file, "c comment").unwrap();
            writeln!(file, "5").unwrap();
        }

        let instances = super::read_instance_list(&instances_file).unwrap();
        assert_eq!(instances, vec![1, 712, 4, 5]);
    }
}
