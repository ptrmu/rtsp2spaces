use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use stakecam::{
    config::{CameraId, Config, TaskId},
    runner::{self, Sources},
    secrets::Secrets,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(
    version,
    about = "Capture configured cameras and collect temperature history"
)]
struct Cli {
    #[arg(long, global = true)]
    verbose: bool,
    #[arg(long, global = true, default_value = "config.toml")]
    config: PathBuf,
    #[arg(long, global = true, default_value = "stake_cam_secrets.json")]
    secrets_file: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    Temperature {
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=48))]
        history_hours: u32,
    },
    Capture {
        #[arg(long, required = true, value_enum)]
        camera: Vec<CameraId>,
    },
}
async fn run(cli: Cli) -> Result<()> {
    let config = Arc::new(Config::load(&cli.config)?);
    let (tasks, history_hours) = match cli.command {
        None => (config.tasks.clone(), 0),
        Some(Command::Capture { camera }) => (camera.into_iter().map(TaskId::from).collect(), 0),
        Some(Command::Temperature { history_hours }) => {
            (vec![TaskId::ChechaquoTemp], history_hours)
        }
    };
    config.validate_tasks(&tasks)?;
    let needs_secrets = tasks.iter().any(|task| match task {
        TaskId::MazamaStakeCan | TaskId::MazamaRanchCam => true,
        TaskId::ChechaquoTemp => config
            .temperature
            .as_ref()
            .is_some_and(|t| t.source == "ambient"),
    });
    let secrets = Arc::new(if needs_secrets {
        Secrets::load(&cli.secrets_file)?
    } else {
        Secrets::default()
    });
    let sources = Arc::new(Sources {
        config: Arc::clone(&config),
        secrets: Arc::clone(&secrets),
        history_hours,
    });
    let report = runner::run(config, tasks, sources).await;
    for path in report.paths {
        println!("{}", path.display());
    }
    for failure in &report.failures {
        eprintln!("error: {}", secrets.redact(failure));
    }
    ensure!(report.failures.is_empty(), "one or more tasks failed");
    Ok(())
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // Only application logging is enabled; protocol and HTTP logs can expose secrets.
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Off)
        .filter_module(
            "stakecam",
            if cli.verbose {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            },
        )
        .init();
    match run(cli).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
