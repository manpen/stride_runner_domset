use structopt::StructOpt;

use super::common::CommonOpts;

#[derive(Debug, StructOpt)]
pub struct InitOpts {}

pub async fn command_init(_common_opts: &CommonOpts, _cmd_opts: &InitOpts) -> anyhow::Result<()> {
    println!("Initializing...");

    Ok(())
}
