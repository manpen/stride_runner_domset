use anyhow::Context;
use console::Style;
use uuid::Uuid;

use crate::utils::directory::StrideDirectory;
use crate::utils::server_connection::ServerConnection;
use crate::utils::settings::global_settings;

use super::arguments::{CommonOpts, RegisterOpts};
use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;

pub async fn command_register(
    common_opts: &CommonOpts,
    cmd_opts: &RegisterOpts,
) -> anyhow::Result<()> {
    // this lock will be kept for relatively long; however, we should reach this
    // section only in a single threaded context --- it is a "main" function of
    // sorts after oll
    let mut global_lock = global_settings()
        .lock()
        .expect("Take lock on global settings");

    if let Some(uuid) = global_lock.solver_uuid {
        if !cmd_opts.delete_old_uuid {
            let style_important = Style::new().red();
            let style_highlight = Style::new().yellow();
            println!("The config file currently contains the following Solver UUID: {uuid}");
            println!(
                "{}",
                style_important.apply_to(
                    "This UUID is required to access previous uploads of results via the website."
                )
            );
            println!(
                "{} {}",
                style_important
                    .apply_to("If you saved this UUID and really want to replace it, use the "),
                style_highlight.apply_to("--delete-old-uuid")
            );
            anyhow::bail!(
                "Solver UUID is present and cannot be replaced without setting --delete-old-uuid"
            );
        }

        save_uuid_to_backup(uuid).with_context(|| "Creating backup of old solver uuid")?;
    }

    let new_uuid = Uuid::new_v4();
    global_lock.solver_uuid = Some(new_uuid);
    global_lock.store_to_path(&StrideDirectory::try_default()?.config_file())?;

    let server_conn = ServerConnection::new_from_opts(common_opts)?;
    let style_success = Style::new().green();
    println!(
        "The new solver uuid is: {}",
        style_success.apply_to(new_uuid)
    );
    println!(
        "Once you recorded a run, you can access the data at:\n  {}",
        style_success.apply_to(server_conn.solver_website_for_user(new_uuid).to_string())
    );

    Ok(())
}

fn save_uuid_to_backup(uuid: Uuid) -> anyhow::Result<()> {
    let path = StrideDirectory::try_default()?
        .data_dir()
        .join("solver_uuid_backup.log");

    let mut file = OpenOptions::new().append(true).create(true).open(path)?;

    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    writeln!(file, "{now} Reregister. The old UUID was {uuid}")?;

    Ok(())
}
