use std::sync::Mutex;

use stride_runner_domset::commands::{
    arguments::*, register::command_register, run::command_run, update::command_update,
};
use structopt::StructOpt;

const LOG_FILE: &str = "stride-runner.log";

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
