use clap::Parser;
use dozer_orchestrator::cli::generate_config_repl;
use dozer_orchestrator::cli::types::{ApiCommands, AppCommands, Cli, Commands, ConnectorCommands};
use dozer_orchestrator::cli::{configure, init_dozer, list_sources, LOGO};
use dozer_orchestrator::errors::OrchestrationError;
use dozer_orchestrator::{set_ctrl_handler, set_panic_hook, Orchestrator};

use dozer_types::log::{error, info};

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use std::{process, thread};
use tokio::runtime::Runtime;

fn main() {
    if let Err(e) = run() {
        error!("{}", e);
        process::exit(1);
    }
}

fn render_logo() {
    use std::println as info;
    const VERSION: &str = env!("CARGO_PKG_VERSION");

    info!("{LOGO}");
    info!("\nDozer Version: {VERSION}\n");
}

fn run() -> Result<(), OrchestrationError> {
    let _tracing_thread = thread::spawn(|| {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            dozer_tracing::init_telemetry(false).unwrap();
        });
    });
    thread::sleep(Duration::from_millis(50));

    set_panic_hook();

    let cli = Cli::parse();
    let running = Arc::new(AtomicBool::new(true));
    set_ctrl_handler(running.clone());
    if let Some(cmd) = cli.cmd {
        // run individual servers
        match cmd {
            Commands::Api(api) => match api.command {
                ApiCommands::Run => {
                    render_logo();
                    let mut dozer = init_dozer(cli.config_path)?;
                    dozer.run_api(running)
                }
                ApiCommands::GenerateToken => {
                    let dozer = init_dozer(cli.config_path)?;
                    let token = dozer.generate_token()?;
                    info!("token: {:?} ", token);
                    Ok(())
                }
            },
            Commands::App(apps) => match apps.command {
                AppCommands::Run => {
                    render_logo();
                    let mut dozer = init_dozer(cli.config_path)?;
                    dozer.run_apps(running, None)
                }
            },
            Commands::Connector(sources) => match sources.command {
                ConnectorCommands::Ls => list_sources(&cli.config_path),
            },
            Commands::Migrate(migrate) => {
                let force = migrate.force.is_some();
                let mut dozer = init_dozer(cli.config_path)?;
                dozer.migrate(force)
            }
            Commands::Clean => {
                let mut dozer = init_dozer(cli.config_path)?;
                dozer.clean()
            }
            Commands::Configure => configure(cli.config_path, running),
            Commands::Init => generate_config_repl(),
        }
    } else {
        render_logo();

        let mut dozer = init_dozer(cli.config_path)?;
        dozer.run_all(running)
    }
}
