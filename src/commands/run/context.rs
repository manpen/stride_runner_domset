use std::collections::HashSet;
use std::{io::BufRead, path::Path};

use chrono::{DateTime, Local};
use rand::seq::SliceRandom;
use sqlx::SqlitePool;
use tracing::debug;
use uuid::Uuid;

use crate::utils::directory::StrideDirectory;
use crate::utils::instance_data_db::InstanceDataDB;
use crate::utils::server_connection::ServerConnection;

use super::super::arguments::{CommonOpts, RunOpts};

pub struct MetaPool(pub SqlitePool);

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

    Ok(instances)
}

async fn check_that_instances_exist(db: &MetaPool, instances: &[u32]) -> anyhow::Result<()> {
    let all_known: HashSet<u32> = fetch_instances_from_db(db, "1=1")
        .await?
        .into_iter()
        .collect();
    let proposed: HashSet<u32> = instances.iter().cloned().collect();

    if !proposed.is_subset(&all_known) {
        let mut difference: Vec<_> = proposed.difference(&all_known).collect();
        difference.sort();
        let len = difference.len();
        let diff_str: Vec<_> = difference
            .into_iter()
            .take(20)
            .map(|iid| iid.to_string())
            .collect();
        let diff_str = diff_str.join(", ");

        anyhow::bail!("List contains {len} instance ids not found in metadata.db; try run `stride-runner update`. At least following IDs were not found {diff_str}");
    }

    Ok(())
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

    Ok(instances)
}

pub struct RunContext {
    common_opts: CommonOpts,
    cmd_opts: RunOpts,

    start: DateTime<Local>,
    run_uuid: Uuid,

    db_meta: MetaPool,

    instance_data_db: InstanceDataDB,
    server_conn: ServerConnection,

    //db_cache: SqlitePool,
    instances: Vec<u32>,

    log_dir: std::path::PathBuf,
}

impl RunContext {
    pub async fn new(common_opts: CommonOpts, cmd_opts: RunOpts) -> anyhow::Result<Self> {
        let stride_dir = StrideDirectory::try_default()?;
        let server_conn = ServerConnection::new_from_opts(&common_opts)?;
        let instance_data_db = InstanceDataDB::new(stride_dir.db_instance_file().as_path()).await?;

        let start = chrono::Local::now();
        let run_uuid = Uuid::new_v4();
        let log_dir = Self::prepare_logdir(&common_opts, start, &run_uuid)?;

        Ok(Self {
            common_opts,
            cmd_opts,

            start,
            run_uuid,

            db_meta: MetaPool(Self::open_db_pool(stride_dir.db_meta_file().as_path()).await?),
            //db_cache: Self::open_db_pool(stride_dir.db_cache_file().as_path()).await?,
            //
            server_conn,
            instance_data_db,
            instances: Vec::new(),

            log_dir,
        })
    }

    fn prepare_logdir(
        common_opts: &CommonOpts,
        start: DateTime<Local>,
        run_uuid: &Uuid,
    ) -> anyhow::Result<std::path::PathBuf> {
        let log_base = &common_opts.run_log_dir;
        let timestamp = start.format("%y%m%d_%H%M%S");
        let dirname = format!("{}_{}", timestamp, run_uuid);

        let path = log_base.join(dirname);
        let _ = std::fs::create_dir_all(&path);

        Ok(path)
    }

    #[allow(dead_code)]
    pub fn common_opts(&self) -> &CommonOpts {
        &self.common_opts
    }

    pub fn cmd_opts(&self) -> &RunOpts {
        &self.cmd_opts
    }

    pub fn db_meta(&self) -> &MetaPool {
        &self.db_meta
    }

    pub fn instance_list(&self) -> &[u32] {
        &self.instances
    }

    #[allow(dead_code)]
    pub fn start(&self) -> DateTime<Local> {
        self.start
    }

    pub fn run_uuid(&self) -> Uuid {
        self.run_uuid
    }

    pub fn server_conn(&self) -> &ServerConnection {
        &self.server_conn
    }

    pub fn instance_data_db(&self) -> &InstanceDataDB {
        &self.instance_data_db
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub async fn build_instance_list(&mut self) -> anyhow::Result<()> {
        if self.cmd_opts.instances.is_none() && self.cmd_opts.sql_where.is_none() {
            anyhow::bail!("Must prove --instances and/or --sql-where");
        }

        let instances_from_file = match &self.cmd_opts.instances {
            Some(path) => {
                let instances = read_instance_list(path.as_path())?;
                debug!("Read {} instances from {:?}", instances.len(), path);
                check_that_instances_exist(self.db_meta(), &instances).await?;
                Some(instances)
            }
            None => None,
        };

        let instances_from_db = match &self.cmd_opts.sql_where {
            Some(where_clause) => {
                let instances = fetch_instances_from_db(self.db_meta(), where_clause).await?;
                debug!(
                    "Read {} instances from InstanceDB where {}",
                    instances.len(),
                    where_clause
                );
                Some(instances)
            }
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

        if self.cmd_opts.sort_instances {
            instance.sort_unstable();
        } else {
            instance.shuffle(&mut rand::thread_rng());
        }

        self.instances = instance;
        Ok(())
    }

    pub fn write_instance_list(&self, path: &Path) -> anyhow::Result<()> {
        use std::io::Write;
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);

        writeln!(
            writer,
            "c {} Instances for STRIDE runner",
            self.instances.len()
        )?;
        for iid in &self.instances {
            writeln!(writer, "{}", iid)?;
        }

        Ok(())
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
