use crate::utils::{
    directory::StrideDirectory, download_progress_bar::DownloadProgressBar,
    instance_data_db::InstanceDataDB, server_connection::ServerConnection,
};
use console::Style;
use indicatif::{MultiProgress, ProgressBar};
use std::{sync::Arc, time::Duration};
use tempdir::TempDir;
use tracing::{debug, info};

use super::arguments::{CommonOpts, UpdateOpts};

const DB_META: &str = "db_meta.db";
const DB_PARTIAL_INSTANCES: &str = "db_partial.db";
const DB_FULL_INSTANCES: &str = "db_full.db";

pub async fn command_update(common_opts: &CommonOpts, cmd_opts: &UpdateOpts) -> anyhow::Result<()> {
    let context = Arc::new(Context {
        cmd_opts: cmd_opts.clone(),
        stride_dir: StrideDirectory::try_default()?,
        server_conn: ServerConnection::new_from_opts(common_opts)?,
        mpb: MultiProgress::new(),
    });

    info!("Start download of metadata database");

    // download meta-data database asynchronously in own tokio task
    let meta_task = tokio::spawn(update_metadata_db(context.clone()));

    // update instance data only if db is missing (typically first run) or user asks for it
    if !context.stride_dir.db_instance_file().exists() || cmd_opts.update_instance_data {
        update_instance_data_db(context).await?;
    }

    meta_task.await??;

    println!("{}", Style::new().green().apply_to("Update complete."));

    Ok(())
}

struct Context {
    cmd_opts: UpdateOpts,
    stride_dir: StrideDirectory,
    server_conn: ServerConnection,
    mpb: MultiProgress,
}

async fn update_metadata_db(context: Arc<Context>) -> anyhow::Result<()> {
    let mut meta_pb = DownloadProgressBar::new(&context.mpb, DB_META.into())?;
    let meta_to_path = context.stride_dir.db_meta_file();

    context
        .server_conn
        .download_file_with_updates(DB_META, meta_to_path.as_path(), &mut meta_pb)
        .await?;

    Ok(())
}

async fn update_instance_data_db(context: Arc<Context>) -> anyhow::Result<()> {
    let instances_name = if context.cmd_opts.all_instances {
        DB_FULL_INSTANCES
    } else {
        DB_PARTIAL_INSTANCES
    };

    let mut instance_pb = DownloadProgressBar::new(&context.mpb, instances_name.into())?;

    let target_db_path = context.stride_dir.db_instance_file();
    let target_exists = target_db_path.exists();

    let (tmpdir, download_path) = if context.cmd_opts.replace_all || !target_exists {
        debug!("Direct download of instance data database");
        // direct download
        if target_exists {
            debug!(" -> Delete existing instance data database");
            std::fs::remove_file(target_db_path.as_path())?;
        }
        (None, target_db_path.clone())
    } else {
        let tempdir = TempDir::new_in(context.stride_dir.data_dir(), "instance-db-download")?;
        let temp_db_path = tempdir.path().join(instances_name);
        debug!(
            "Download instance data database to temporary location: {:?}",
            temp_db_path
        );
        (Some(tempdir), temp_db_path)
    };

    context
        .server_conn
        .download_file_with_updates(
            DB_PARTIAL_INSTANCES,
            download_path.as_path(),
            &mut instance_pb,
        )
        .await?;

    debug!("Instance data database downloaded");

    if tmpdir.is_none() {
        // direct download
        return Ok(());
    }

    std::mem::drop(instance_pb);

    let progress = context.mpb.add(ProgressBar::new_spinner());
    progress.set_message("Merging instance data databases");
    progress.enable_steady_tick(Duration::from_millis(50));

    debug!("Start merging instance data databases");
    let target_db = InstanceDataDB::new(target_db_path.as_path()).await?;
    target_db.add_from_db_file(download_path.as_path()).await?;

    std::mem::drop(tmpdir);

    Ok(())
}
