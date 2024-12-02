use structopt::StructOpt;

use super::common::CommonOpts;

#[derive(Debug, StructOpt)]
pub struct RegisterOpts {}

pub async fn command_register(
    _common_opts: &CommonOpts,
    _cmd_opts: &RegisterOpts,
) -> anyhow::Result<()> {
    Ok(())
}
