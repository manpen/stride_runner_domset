use std::{path::PathBuf, sync::LazyLock, time::Duration};

use reqwest::Url;
use structopt::StructOpt;
use tracing::level_filters::LevelFilter;
use uuid::Uuid;

use crate::utils::settings::global_settings;

#[derive(StructOpt)]
pub enum RegisterEnum {
    Register(RegisterOpts),
}

#[derive(StructOpt)]
pub enum UpdateEnum {
    Update(UpdateOpts),
}

#[derive(StructOpt)]
pub enum RunEnum {
    Run(RunOpts),
}

#[derive(StructOpt)]
#[allow(clippy::enum_variant_names)]
pub enum Commands {
    #[structopt(flatten)]
    RegisterEnum(RegisterEnum),

    #[structopt(flatten)]
    UpdateEnum(UpdateEnum),

    #[structopt(flatten)]
    RunEnum(RunEnum),
}

#[derive(StructOpt)]
pub struct Arguments {
    #[structopt(flatten)]
    pub common: CommonOpts,

    #[structopt(subcommand)]
    pub cmd: Commands,
}

////////////////////

macro_rules! gen_default {
    ($name:ident, $field:ident) => {
        paste::paste! {
            static [< DEFAULT_ $name >] : LazyLock<String> =
            LazyLock::new(|| global_settings().lock().unwrap().$field.to_string() );
        }
    };
}

gen_default!(SERVER_URL, server_url);
gen_default!(RUN_LOG_DIR, run_log_dir);

#[derive(Clone, Debug, StructOpt)]
pub struct CommonOpts {
    #[structopt(
        short,
        long,
        help = "Enable logging to file. Possible values: info < debug < trace"
    )]
    pub logging: Option<LevelFilter>,

    #[structopt(long, help = "Path where logs are kept", default_value = &DEFAULT_RUN_LOG_DIR)]
    pub run_log_dir: PathBuf,

    #[structopt(
        long,
        help = "Server URL (without path!)",
        default_value = &DEFAULT_SERVER_URL
    )]
    pub server_url: Url,
}

impl CommonOpts {
    pub fn server_url(&self) -> &Url {
        &self.server_url
    }
}

////////////////////

gen_default!(SOLVER_BIN, solver_bin);
gen_default!(TIMEOUT, timeout);
gen_default!(GRACE, grace);
gen_default!(PARALLEL_JOBS, parallel_jobs);

#[derive(Clone, Debug, StructOpt)]
pub struct RunOpts {
    #[structopt(
        short = "-b",
        long = "solver-bin",
        help = "Path to the solver binary to be executed",
        default_value = &DEFAULT_SOLVER_BIN
    )]
    pub solver_binary: PathBuf,

    #[structopt(
        short = "-S",
        long,
        help = "UUID of the solver to be used; enables upload of all results for later analysis. If omitted use value from config."
    )]
    pub solver_uuid: Option<Uuid>,

    #[structopt(
        short = "-T",
        long,
        help = "Send SIGTERM after that many seconds",
        default_value = &DEFAULT_TIMEOUT
    )]
    pub timeout: u64,

    #[structopt(
        short = "-G",
        long,
        help = "Kill solver after that many seconds after SIGTERM",
        default_value = &DEFAULT_GRACE
    )]
    pub grace: u64,

    #[structopt(short = "-j", long, help = "Max. number of parallel solver runs", default_value=&DEFAULT_PARALLEL_JOBS)]
    pub parallel_jobs: usize,

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

/////////////////////

#[derive(Debug, StructOpt)]
pub struct RegisterOpts {}

/////////////////////

#[derive(Debug, StructOpt)]
pub struct UpdateOpts {
    #[structopt(short, long, help = "WARNING: requires more than 10GB of storage")]
    pub all_instances: bool,
}