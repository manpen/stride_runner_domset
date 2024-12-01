use std::{sync::Arc, time::Duration};

use rand::{seq::SliceRandom as _, Rng as _};
use tokio::sync::Mutex;

use super::context::RunContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerResult {
    Optimal,
    Suboptimal,
    Infeasible,
    Incomplete,
    Error,
    Timeout,
}

pub struct Runner {
    context: Arc<RunContext>,
    iid: u32,
    result: Mutex<Option<RunnerResult>>,
}

impl Runner {
    pub fn new(context: Arc<RunContext>, iid: u32) -> Self {
        Self {
            context,
            iid,
            result: Mutex::new(None),
        }
    }

    pub async fn main(&self) {
        let seconds = rand::thread_rng().gen_range(
            self.context.cmd_opts().timeout / 4
                ..=(self.context.cmd_opts().timeout + self.context.cmd_opts().grace),
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
