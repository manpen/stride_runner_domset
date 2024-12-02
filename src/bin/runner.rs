use std::sync::Mutex;

use stride_runner_domset::commands::{
    common::CommonOpts,
    register::{command_register, RegisterOpts},
    run::{command_run, RunOpts},
    update::{command_update, UpdateOpts},
};
use structopt::StructOpt;

const LOG_FILE: &str = "stride-runner.log";

#[derive(StructOpt)]
enum RegisterEnum {
    Register(RegisterOpts),
}

#[derive(StructOpt)]
enum UpdateEnum {
    Update(UpdateOpts),
}

#[derive(StructOpt)]
enum RunEnum {
    Run(RunOpts),
}

#[derive(StructOpt)]
#[allow(clippy::enum_variant_names)]
enum Commands {
    #[structopt(flatten)]
    RegisterEnum(RegisterEnum),

    #[structopt(flatten)]
    UpdateEnum(UpdateEnum),

    #[structopt(flatten)]
    RunEnum(RunEnum),
}

#[derive(StructOpt)]
struct Arguments {
    #[structopt(flatten)]
    common: CommonOpts,

    #[structopt(subcommand)]
    cmd: Commands,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Arguments::from_args();

    if let Some(level) = opts.common.logging {
        println!("Enabled logging to file {LOG_FILE} with level {level:?}");
        let file = std::fs::File::create(LOG_FILE)?;
        tracing_subscriber::fmt()
            .with_max_level(level)
            .with_writer(Mutex::new(file))
            .init();
    };

    match opts.cmd {
        Commands::RegisterEnum(RegisterEnum::Register(cmd_opts)) => {
            command_register(&opts.common, &cmd_opts).await?
        }
        Commands::UpdateEnum(UpdateEnum::Update(cmd_opts)) => {
            command_update(&opts.common, &cmd_opts).await?
        }
        Commands::RunEnum(RunEnum::Run(cmd_opts)) => command_run(&opts.common, &cmd_opts).await?,
    }

    Ok(())
}
