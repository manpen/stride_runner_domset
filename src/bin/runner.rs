use std::sync::Mutex;

use stride_runner_domset::{
    commands::{
        arguments::*, register::command_register, run::command_run, update::command_update,
    },
    utils::{directory::StrideDirectory, settings::Settings},
};
use structopt::StructOpt;
use tracing::debug;

const LOG_FILE: &str = "stride-runner.log";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = read_and_register_settings()?; // must happen BEFORE `Arguments::from_args()` !!!
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
        Commands::RunEnum(RunEnum::Run(mut cmd_opts)) => {
            if cmd_opts.solver_binary.to_string_lossy().len() == 0 {
                anyhow::bail!("Missing solver binary; please set --solver-bin");
            }

            if cmd_opts.solver_uuid.is_none() {
                if let Some(uuid) = &settings.solver_uuid {
                    cmd_opts.solver_uuid = Some(*uuid);
                }
            }

            command_run(&opts.common, &cmd_opts).await?
        }
    }

    Ok(())
}

fn read_and_register_settings() -> anyhow::Result<Settings> {
    // try read config file and store it to global var, which will be used by the
    // Arguments parser. This is not a nice design, but I'm not aware of any better
    // approach using StructOpt.
    if let Ok(settings) = Settings::load_from_default_path() {
        debug!("Read settings from file: {settings:?}");
        settings.store_to_global_variable()?;
        return Ok(settings);
    }

    let settings = Settings::default();
    let path = StrideDirectory::try_default()?.config_file();
    if path.is_file() {
        anyhow::bail!("Could not read settings at {path:?}. Check syntax");
    } else {
        settings.store_to_path(path.as_path())?;
    }

    Ok(settings)
}
