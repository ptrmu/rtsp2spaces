use anyhow::Result;
use clap::{Parser, Subcommand};
use stakecam::{camera::Camera, config::Config, secrets::Secrets};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Capture a Reolink camera snapshot")]
struct Cli {
    #[arg(long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Capture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        secrets_file: PathBuf,
    },
}
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Capture {
            config,
            secrets_file,
        } => {
            let config = Config::load(&config)?;
            let secrets = Secrets::load(&secrets_file)?;
            let camera = Camera {
                config: &config.camera,
                secrets: &secrets,
            };
            log::debug!("starting snapshot capture");
            let capture = stakecam::capture(&camera, &config, chrono::Utc::now())?;
            println!("{}", capture.path.display());
        }
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // Do not enable reqwest/hyper tracing: authenticated URLs must stay private.
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
    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
