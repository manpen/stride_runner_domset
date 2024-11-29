use log::debug;
use sqlx::SqlitePool;
use std::{
    io::BufRead,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use uuid::Uuid;

use super::common::CommonOpts;

#[derive(Clone, Debug, StructOpt)]
pub struct RunOpts {
    #[structopt(short = "-S", long)]
    solver_uuid: Option<Uuid>,

    #[structopt(short = "-T", long)]
    timeout: Option<u64>,

    #[structopt(short = "-G", long)]
    grace: Option<u64>,

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

struct RunContext {
    common_opts: CommonOpts,
    cmd_opts: RunOpts,

    db_meta: SqlitePool,
    db_instances: SqlitePool,
    //db_cache: SqlitePool,
}

impl RunContext {
    pub async fn new(common_opts: CommonOpts, cmd_opts: RunOpts) -> anyhow::Result<Self> {
        let stride_dir = common_opts.stride_dir()?;

        Ok(Self {
            common_opts,
            cmd_opts,

            db_meta: Self::open_db_pool(stride_dir.db_meta_file().as_path()).await?,
            db_instances: Self::open_db_pool(stride_dir.db_instance_file().as_path()).await?,
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

    pub fn run(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub async fn command_run(common_opts: &CommonOpts, cmd_opts: &RunOpts) -> anyhow::Result<()> {
    let context = RunContext::new(common_opts.clone(), cmd_opts.clone()).await?;

    let instances_from_file = match &cmd_opts.instances {
        Some(path) => Some(read_instance_list(path.as_path())?),
        None => None,
    };

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
