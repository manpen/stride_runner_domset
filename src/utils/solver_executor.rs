use std::{fs::File, path::PathBuf, process::ExitStatus, time::Duration};

use anyhow::Context;
use derive_builder::Builder;
use serde::Serialize;
use std::io::BufReader;
use tokio::{
    process::{Child, Command},
    time::{timeout, Instant},
};
use tracing::{debug, trace};

use crate::pace::{graph::Node, instance_reader::PaceReader, Solution};

use super::IId;

#[derive(Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum SolverResult {
    Valid { data: Vec<Node> },
    ValidCached,
    Infeasible,
    SyntaxError, // TODO: distinguish between syntax and runner errors
    Timeout,
    IncompleteOutput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChildExitCode {
    BeforeTimeout(ExitStatus),
    WithinGrace(ExitStatus),
    Timeout,
}

impl SolverResult {
    pub fn score(&self) -> Option<u32> {
        match self {
            SolverResult::Valid { data } => Some(data.len() as u32),
            _ => None,
        }
    }
}

#[derive(Debug, Builder)]
pub struct SolverExecutor {
    working_dir: PathBuf,
    solver_path: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,

    timeout: Duration,
    grace: Duration,

    #[builder(setter(skip))]
    runtime: Option<Duration>,

    instance_id: IId,
    instance_data: String,
}

const PATH_STDIN: &str = "stdin.gr";
const PATH_STDOUT: &str = "stdout";
const PATH_STDERR: &str = "stderr";

impl SolverExecutor {
    pub async fn run(&mut self) -> anyhow::Result<SolverResult> {
        self.move_instance_data_to_file()?;

        // spawn and execute solver as child
        let start_time = Instant::now();
        let child = self.spawn_child()?;
        let wait_result = self.timeout_wait_for_child_to_complete(child).await?;
        self.runtime = Some(start_time.elapsed());

        let status = match wait_result {
            ChildExitCode::BeforeTimeout(status) => status,
            ChildExitCode::WithinGrace(status) => {
                if status.success() {
                    status
                } else {
                    return Ok(SolverResult::Timeout);
                }
            }
            ChildExitCode::Timeout => return Ok(SolverResult::Timeout),
        };

        // TODO: we might want to handle a non-zero exit status differently
        if !status.success() {
            return Ok(SolverResult::SyntaxError);
        }

        self.verify_solution()
    }

    pub fn delete_files(&self) -> anyhow::Result<()> {
        let stdin = self.filename(PATH_STDIN);
        let stdout = self.filename(PATH_STDOUT);
        let stderr = self.filename(PATH_STDERR);

        if stdin.exists() {
            // we may not have an stdin file
            std::fs::remove_file(stdin)?;
        }
        std::fs::remove_file(stdout)?;
        std::fs::remove_file(stderr)?;

        Ok(())
    }

    pub fn runtime(&self) -> Option<Duration> {
        self.runtime
    }

    fn verify_solution(&self) -> anyhow::Result<SolverResult> {
        let instance_file = BufReader::new(File::open(self.filename(PATH_STDIN))?);
        let instance_reader = PaceReader::try_new(instance_file)?;
        let n = instance_reader.number_of_nodes();
        let mut edges = Vec::with_capacity(instance_reader.number_of_edges() as usize);
        for edge in instance_reader {
            edges.push(edge?);
        }

        let solution_file = BufReader::new(File::open(self.filename(PATH_STDOUT))?);
        let solution = match Solution::read(solution_file, Some(n)) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(SolverResult::IncompleteOutput);
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                return Ok(SolverResult::SyntaxError);
            }
            Err(e) => return Err(e.into()),
        };

        match solution.valid_domset_for_instance(n, edges.into_iter()) {
            Ok(true) => anyhow::Ok(SolverResult::Valid {
                data: solution.take_1indexed_solution(),
            }),
            Ok(false) => anyhow::Ok(SolverResult::Infeasible),
            Err(e) => Err(e.into()),
        }
    }

    fn filename(&self, suffix: &str) -> PathBuf {
        self.working_dir
            .join(format!("iid{}.{}", self.instance_id.iid_to_u32(), suffix))
    }

    fn move_instance_data_to_file(&mut self) -> anyhow::Result<()> {
        let path = self.filename(PATH_STDIN);
        std::fs::write(&path, &self.instance_data)?;
        std::mem::take(&mut self.instance_data); // free allocation
        Ok(())
    }

    fn spawn_child(&mut self) -> Result<Child, anyhow::Error> {
        let stdin = File::open(self.filename(PATH_STDIN)).with_context(|| "Open STDIN")?;
        let stdout = File::create(self.filename(PATH_STDOUT)).with_context(|| "Open STDOUT")?;
        let stderr = File::create(self.filename(PATH_STDERR)).with_context(|| "Open STDERR")?;

        trace!(
            "Spawn solver {:?} with args {:?}",
            self.solver_path,
            &self.args
        );
        let child = Command::new(&self.solver_path)
            .args(&self.args)
            .envs(self.env.iter().cloned())
            .stdin(stdin)
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .with_context(|| "Spawn solver as child")?;
        Ok(child)
    }

    /// In case of no error, we return
    ///  - Some(ExitStatus) if the child has exited
    ///  - None if the child has been killed using SIGKILL
    async fn timeout_wait_for_child_to_complete(
        &self,
        mut child: Child,
    ) -> anyhow::Result<ChildExitCode> {
        // we get an error if we run into the timeout
        if let Ok(res) = timeout(self.timeout, child.wait()).await {
            return Ok(ChildExitCode::BeforeTimeout(res?));
        }

        debug!(
            "{:?} Timeout after {}s reached; send sigterm child",
            self.instance_id,
            self.timeout.as_secs()
        );

        // send SIGTERM to the child (we use unsafe here, because I do not want to pull a crate for this one line)
        if let Some(pid) = child.id() {
            // we only get None if the child has already exited
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        // issue a grace period
        if !self.grace.is_zero() {
            if let Ok(res) = timeout(self.grace, child.wait()).await {
                return Ok(ChildExitCode::WithinGrace(res?));
            }
        }

        debug!(
            "{:?} Grace period after {}s reached; kill child",
            self.instance_id,
            self.timeout.as_secs()
        );

        child.kill().await?;

        Ok(ChildExitCode::Timeout)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::{Path, PathBuf};
    use tempdir::TempDir;

    const BIN_DUMMY: &str = "test_dummy";
    const BIN_GREEDY: &str = "greedy";

    const TIMEOUT_MS: u64 = 1000;
    const GRACE_MS: u64 = 500;

    const REF_ID: IId = IId::new(1582);
    const REF_DATA: &str = "p ds 9 8\n1 3\n1 4\n1 7\n2 8\n3 9\n4 8\n4 9\n5 6\n";

    const PREFIX: &str = "stride-solver-executor-test";

    fn exec_path(binary: &str) -> PathBuf {
        for mode in ["release", "debug"] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(Path::new(mode))
                .join("examples")
                .join(Path::new(binary));

            if path.exists() {
                return path;
            }
        }

        panic!(
            "Could not find dummy executable; please run `cargo build [--release] {binary}` first"
        );
    }

    fn default_test_executor(binary: &str, args: Vec<String>) -> (TempDir, SolverExecutor) {
        use std::time::Duration;

        let tmp_dir = TempDir::new(PREFIX).unwrap();
        let workdir = tmp_dir.path().to_path_buf();
        (
            tmp_dir,
            super::SolverExecutorBuilder::default()
                .solver_path(exec_path(binary))
                .working_dir(workdir)
                .args(args)
                .timeout(Duration::from_millis(TIMEOUT_MS))
                .grace(Duration::from_millis(GRACE_MS))
                .instance_id(REF_ID)
                .instance_data(REF_DATA.into())
                .env(Vec::new())
                .build()
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn test_normal_exit() {
        #[allow(unused)]
        let (tmp_dir, mut exec) = default_test_executor(BIN_DUMMY, vec!["normal".into()]);
        exec.move_instance_data_to_file().unwrap();

        let start = std::time::Instant::now();
        let child = exec.spawn_child().unwrap();
        let status = match exec
            .timeout_wait_for_child_to_complete(child)
            .await
            .unwrap()
        {
            ChildExitCode::BeforeTimeout(x) => x,
            _ => panic!("Not supposed to happen"),
        };

        assert!(
            start.elapsed().as_millis() < 1000,
            "{:?}ms",
            start.elapsed()
        );

        assert!(status.success(), "{status:?}");
    }

    #[tokio::test]
    async fn test_sigterm() {
        #[allow(unused)]
        let (tmp_dir, mut exec) = default_test_executor(BIN_DUMMY, vec!["sig-term".into()]);
        exec.move_instance_data_to_file().unwrap();

        let start = std::time::Instant::now();
        let child = exec.spawn_child().unwrap();
        let status = match exec
            .timeout_wait_for_child_to_complete(child)
            .await
            .unwrap()
        {
            ChildExitCode::WithinGrace(x) => x,
            _ => panic!("Not supposed to happen"),
        };

        assert!(
            start.elapsed().as_millis() > TIMEOUT_MS as u128,
            "{:?}ms",
            start.elapsed()
        );

        assert!(status.success(), "{status:?}");
    }

    #[tokio::test]
    async fn test_kill() {
        #[allow(unused)]
        let (tmp_dir, mut exec) = default_test_executor(BIN_DUMMY, vec!["never-terminate".into()]);
        exec.move_instance_data_to_file().unwrap();

        let start = std::time::Instant::now();
        let child = exec.spawn_child().unwrap();
        let status = exec
            .timeout_wait_for_child_to_complete(child)
            .await
            .unwrap();

        assert!(
            start.elapsed().as_millis() > (TIMEOUT_MS + GRACE_MS) as u128,
            "{:?}ms",
            start.elapsed()
        );

        // timeout yields None if the child has been killed
        assert_eq!(status, ChildExitCode::Timeout);
    }

    #[tokio::test]
    async fn test_run_greedy_ok() {
        for args in [vec![], vec!["-c"], vec!["-c", "-e"], vec!["-c", "-e", "-t"]] {
            #[allow(unused)]
            let (tmp_dir, mut exec) = default_test_executor(
                BIN_GREEDY,
                args.into_iter().map(|s| s.to_string()).collect(),
            );
            let status = exec.run().await.unwrap();

            match status {
                crate::utils::solver_executor::SolverResult::Valid { .. } => {}
                _ => panic!("Unexpected result: {:?}", status),
            }
        }
    }

    #[tokio::test]
    async fn test_run_greedy_infeasible() {
        #[allow(unused)]
        let (tmp_dir, mut exec) = default_test_executor(BIN_GREEDY, vec!["-i".into()]);
        let status = exec.run().await.unwrap();

        match status {
            crate::utils::solver_executor::SolverResult::Infeasible => {}
            _ => panic!("Unexpected result: {:?}", status),
        }
    }

    #[tokio::test]
    async fn test_run_greedy_timeout() {
        #[allow(unused)]
        let (tmp_dir, mut exec) = default_test_executor(BIN_GREEDY, vec!["-n".into()]);
        let status = exec.run().await.unwrap();

        match status {
            crate::utils::solver_executor::SolverResult::Timeout => {}
            _ => panic!("Unexpected result: {:?}", status),
        }
    }
}
