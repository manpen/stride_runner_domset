use std::sync::Mutex;

use anyhow::Context;
use console::Style;
use stride_runner_domset::{
    commands::{
        arguments::*,
        export::{command_export_instance, command_export_solution},
        import::command_import_solution,
        register::command_register,
        run::command_run,
        update::command_update,
    },
    utils::{directory::StrideDirectory, settings::Settings},
};
use structopt::StructOpt;
use tracing::debug;

const LOG_FILE: &str = "stride-runner.log";

fn parse_arguments() -> anyhow::Result<(Arguments, Vec<String>)> {
    let mut arg_iter = std::env::args();
    let opts = Arguments::from_iter_safe(arg_iter.by_ref().take_while(|arg| arg != "--"))?;
    let remaining = arg_iter.collect();

    Ok((opts, remaining))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = read_and_register_settings()?; // must happen BEFORE `Arguments::from_args()` !!!
    let (opts, child_args) = parse_arguments()?;

    if let Some(level) = opts.common.logging {
        println!("Enabled logging to file {LOG_FILE} with level {level:?}");
        let file = std::fs::File::create(LOG_FILE)?;
        tracing_subscriber::fmt()
            .with_max_level(level)
            .with_writer(Mutex::new(file))
            .init();
    };

    debug!("Parsed arguments with solver args: {child_args:?}");

    let result = match opts.cmd {
        Commands::RegisterEnum(RegisterEnum::Register(cmd_opts)) => {
            command_register(&opts.common, &cmd_opts).await
        }
        Commands::UpdateEnum(UpdateEnum::Update(mut cmd_opts)) => {
            cmd_opts.update_instance_data |= cmd_opts.replace_all | cmd_opts.all_instances;
            command_update(&opts.common, &cmd_opts).await
        }
        Commands::RunEnum(RunEnum::Run(mut cmd_opts)) => {
            if cmd_opts.solver_binary.to_string_lossy().is_empty() {
                anyhow::bail!("Missing solver binary; please set --solver-bin");
            }

            cmd_opts.solver_args = child_args;

            if cmd_opts.solver_uuid.is_none() {
                if let Some(uuid) = &settings.solver_uuid {
                    cmd_opts.solver_uuid = Some(*uuid);
                }
            }

            command_run(&opts.common, &cmd_opts).await
        }
        Commands::ExportInstanceEnum(ExportInstanceEnum::ExportInstance(cmd_opts)) => {
            command_export_instance(&opts.common, &cmd_opts).await
        }
        Commands::ExportSolutionEnum(ExportSolutionEnum::ExportSolution(cmd_opts)) => {
            command_export_solution(&opts.common, &cmd_opts).await
        }
        Commands::ImportSolutionEnum(ImportSolutionEnum::ImportSolution(cmd_opts)) => {
            command_import_solution(&opts.common, &cmd_opts).await
        }
    };

    if let Err(e) = result {
        debug!("Error: {e}");
        println!("{}: {e}", Style::new().red().bold().apply_to("Error"));
        std::process::exit(1);
    }

    Ok(())
}

fn read_and_register_settings() -> anyhow::Result<Settings> {
    // try read config file and store it to global var, which will be used by the
    // Arguments parser. This is not a nice design, but I'm not aware of any better
    // approach using StructOpt.
    let path = StrideDirectory::try_default()?.config_file();

    if !path.is_file() {
        let style = Style::new().blue();
        println!(
            "{} {:?}",
            style.apply_to("Did not detect a config file. Created template "),
            path
        );
        println!("Have a look at it --- it may save you some work later on ;)");

        let settings = Settings::default();
        settings.store_to_path(path.as_path())?;
    }

    let settings = Settings::load_from_default_path()
        .with_context(|| format!("Reading settings from {path:?}"))?;
    debug!("Read settings from file: {settings:?}");
    settings.store_to_global_variable()?;

    Ok(settings)
}
