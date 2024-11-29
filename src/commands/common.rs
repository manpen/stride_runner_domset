use std::path::PathBuf;

use reqwest::Url;
use structopt::StructOpt;

use crate::utils::directory::StrideDirectory;

#[derive(Clone, Debug, StructOpt)]
pub struct CommonOpts {
    #[structopt(short, long, help = "Prints debug information")]
    pub debug: bool,

    #[structopt(
        long,
        help = "Path to the data directory; CHANGE WITH CAUTION",
        default_value = ".stride"
    )]
    pub data_dir: PathBuf,

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
