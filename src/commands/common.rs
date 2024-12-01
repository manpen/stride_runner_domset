use std::path::PathBuf;

use reqwest::Url;
use structopt::StructOpt;
use tracing::level_filters::LevelFilter;

use crate::utils::directory::StrideDirectory;

#[derive(Clone, Debug, StructOpt)]
pub struct CommonOpts {
    #[structopt(
        short,
        long,
        help = "Enable logging to file. Possible values: info < debug < trace"
    )]
    pub logging: Option<LevelFilter>,

    #[structopt(
        long,
        help = "Path to the data directory; CHANGE WITH CAUTION",
        default_value = ".stride"
    )]
    pub data_dir: PathBuf,

    #[structopt(long, help = "Path where logs are kept", default_value = "stride-logs")]
    pub run_log_dir: PathBuf,

    #[structopt(
        long,
        help = "Server URL (without path!)",
        default_value = "https://domset.algorithm.engineering"
    )]
    pub server_url: Url,
}

impl CommonOpts {
    pub fn stride_dir(&self) -> anyhow::Result<StrideDirectory> {
        StrideDirectory::try_new(self.data_dir.clone())
    }

    pub fn server_url(&self) -> &Url {
        &self.server_url
    }
}
