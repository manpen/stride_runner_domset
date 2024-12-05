use std::path::Path;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::utils::{
    download_progress_bar::DownloadProgressBar, server_connection::ServerConnection,
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
    let server_conn = ServerConnection::new_from_opts(common_opts)?;
    let search_path = format!("api/instances/download/{}", cmd_opts.instance);
    download(
        server_conn,
        &search_path,
        cmd_opts.output.as_path(),
        cmd_opts.force,
    )
    .await
}

pub async fn command_export_solution(
    common_opts: &CommonOpts,
    cmd_opts: &ExportSolutionOpts,
) -> anyhow::Result<()> {
    let server_conn = ServerConnection::new_from_opts(common_opts)?;
    let search_path = format!(
        "api/solutions/download?iid={}&solver={}&run={}",
        cmd_opts.instance, cmd_opts.solver, cmd_opts.run
    );

    download(
        server_conn,
        &search_path,
        cmd_opts.output.as_path(),
        cmd_opts.force,
    )
    .await
}
