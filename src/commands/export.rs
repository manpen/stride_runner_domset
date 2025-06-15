use std::{
    io::Write,
    path::{Path, PathBuf},
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::utils::{
    directory::StrideDirectory, download_progress_bar::DownloadProgressBar,
    instance_data_db::InstanceDataDB, meta_data_db::MetaDataDB,
    server_connection::ServerConnection,
};

use super::arguments::{CommonOpts, ExportInstanceOpts, ExportSolutionOpts};

async fn download(
    server_conn: ServerConnection,
    url_search_path: &str,
    destination: &Path,
    force: bool,
) -> anyhow::Result<()> {
    if !force && destination.exists() {
        anyhow::bail!(
            "File already exists: {}; change output path or use -f/--force to overwrite",
            destination.display()
        );
    }

    {
        let mpb = MultiProgress::new();
        let line = mpb.add(ProgressBar::no_length());
        line.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());
        line.set_message(format!("Downloading to {}", destination.display()));

        let mut download_progress = DownloadProgressBar::new(
            &mpb,
            destination
                .file_name()
                .map_or(String::new(), |p| p.to_string_lossy().to_string()),
        )?;

        server_conn
            .download_file_with_updates(url_search_path, destination, &mut download_progress)
            .await?;
    }

    println!("Downloaded to: {}", destination.display());

    Ok(())
}

pub async fn command_export_instance(
    common_opts: &CommonOpts,
    cmd_opts: &ExportInstanceOpts,
) -> anyhow::Result<()> {
    let stride_dir = StrideDirectory::try_default()?;
    let server_conn = ServerConnection::new_from_opts(common_opts)?;
    let instance_data_db = InstanceDataDB::new(stride_dir.db_instance_file().as_path()).await?;
    let meta_db = MetaDataDB::new(stride_dir.db_meta_file().as_path()).await?;
    let data = instance_data_db
        .fetch_data(&server_conn, &meta_db, cmd_opts.instance)
        .await?;

    let destination = if let Some(path) = cmd_opts.output.clone() {
        if path == PathBuf::from("/") {
            Path::new(&cmd_opts.instance.iid_to_u32().to_string())
                .with_extension("gr")
                .to_path_buf()
        } else {
            path
        }
    } else {
        std::io::stdout().lock().write_all(data.as_bytes())?;
        return Ok(());
    };

    if !cmd_opts.force && destination.is_file() {
        anyhow::bail!(
            "File already exists: {}; change output path or use -f/--force to overwrite",
            destination.display()
        );
    }

    let mut file = std::fs::File::create(&destination)?;
    file.write_all(data.as_bytes())?;

    println!("Stored instance data to: {}", destination.display());
    Ok(())
}

pub async fn command_export_solution(
    common_opts: &CommonOpts,
    cmd_opts: &ExportSolutionOpts,
) -> anyhow::Result<()> {
    let server_conn = ServerConnection::new_from_opts(common_opts)?;
    let search_path = format!(
        "api/solutions/download?iid={}&solver={}&run={}",
        cmd_opts.instance.iid_to_u32(),
        cmd_opts.solver,
        cmd_opts.run
    );

    download(
        server_conn,
        &search_path,
        cmd_opts.output.as_path(),
        cmd_opts.force,
    )
    .await
}
