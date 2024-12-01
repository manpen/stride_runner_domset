use crate::utils::server_connection::{
    DownloadProgress, DownloadProgressCallback, ServerConnection,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Arc;
use structopt::StructOpt;
use tracing::info;

use super::common::CommonOpts;

const DB_META: &str = "db_meta.db";
const DB_PARTIAL_INSTANCES: &str = "db_partial.db";
const DB_FULL_INSTANCES: &str = "db_full.db";

#[derive(Debug, StructOpt)]
pub struct UpdateOpts {
    #[structopt(short, long, help = "WARNING: requires more than 10GB of storage")]
    all_instances: bool,
}

struct DownloadProgressBar {
    pb: ProgressBar,
}

impl DownloadProgressBar {
    fn new(parent: &MultiProgress, name: String) -> anyhow::Result<Self> {
        let pb = parent.add(ProgressBar::no_length());
        pb.set_style(ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")?
        .progress_chars("#>-"));

        pb.set_message(name);

        Ok(Self { pb })
    }
}

impl DownloadProgressCallback for DownloadProgressBar {
    fn init(&mut self, total_size: Option<u64>) {
        if let Some(size) = total_size {
            self.pb.set_length(size);
        }
    }

    fn update(&mut self, state: DownloadProgress) {
        self.pb.set_position(state.downloaded);
    }
}

pub async fn command_update(common_opts: &CommonOpts, cmd_opts: &UpdateOpts) -> anyhow::Result<()> {
    let data_dir = common_opts.stride_dir()?;

    let server_conn = Arc::new(ServerConnection::new_from_opts(common_opts)?);

    info!("Start download of metadata database");

    let instances_name = if cmd_opts.all_instances {
        DB_FULL_INSTANCES
    } else {
        DB_PARTIAL_INSTANCES
    };

    let mpb = MultiProgress::new();
    let mut meta_pb = DownloadProgressBar::new(&mpb, DB_META.into())?;
    let mut instance_pb = DownloadProgressBar::new(&mpb, instances_name.into())?;

    let meta_to_path = data_dir.db_meta_file();
    let meta_server_conn = server_conn.clone();
    let meta_task = tokio::spawn(async move {
        meta_server_conn
            .download_file_with_updates(DB_META, meta_to_path.as_path(), &mut meta_pb)
            .await
            .unwrap();
    });

    server_conn
        .download_file_with_updates(
            instances_name,
            data_dir.db_instance_file().as_path(),
            &mut instance_pb,
        )
        .await
        .unwrap();

    meta_task.await.unwrap();

    Ok(())
}
