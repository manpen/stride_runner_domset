use crate::utils::{
    directory::StrideDirectory, download_progress_bar::DownloadProgressBar,
    server_connection::ServerConnection,
};
use console::Style;
use indicatif::MultiProgress;
use std::sync::Arc;
use tracing::info;

use super::arguments::{CommonOpts, UpdateOpts};

const DB_META: &str = "db_meta.db";
const DB_PARTIAL_INSTANCES: &str = "db_partial.db";
const DB_FULL_INSTANCES: &str = "db_full.db";

pub async fn command_update(common_opts: &CommonOpts, cmd_opts: &UpdateOpts) -> anyhow::Result<()> {
    let data_dir = StrideDirectory::try_default()?;

    let server_conn = Arc::new(ServerConnection::new_from_opts(common_opts)?);

    info!("Start download of metadata database");

    let instances_name = if cmd_opts.all_instances {
        DB_FULL_INSTANCES
    } else {
        DB_PARTIAL_INSTANCES
    };

    {
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
    }

    println!("{}", Style::new().green().apply_to("Update complete."));

    Ok(())
}
