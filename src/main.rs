use anyhow::Result;
use clap::{Parser, Subcommand};
use stakecam::{camera::Camera, config::Config, secrets::Secrets};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Capture camera snapshots and collect temperature history"
)]
struct Cli {
    #[arg(long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Temperature {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        secrets_file: PathBuf,
        /// Import recent history in addition to the latest reading (maximum 24 hours).
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=24))]
        history_hours: u32,
    },
    Capture {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        secrets_file: PathBuf,
    },
}
fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Temperature {
            config,
            secrets_file,
            history_hours,
        } => {
            let config = Config::load(&config)?;
            let settings = config
                .temperature
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing [temperature] configuration"))?;
            let now = chrono::Utc::now();
            let (_, paths) = match settings.source.as_str() {
                "ambient" => {
                    let secrets = Secrets::load(&secrets_file)?;
                    stakecam::temperature::collect(
                        &stakecam::temperature::Ambient {
                            config: settings,
                            secrets: &secrets,
                        },
                        &config.data_dir,
                        now,
                        history_hours,
                    )?
                }
                "fixture" => stakecam::temperature::collect(
                    &stakecam::temperature::FixtureSource(
                        settings
                            .fixture_file
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("missing fixture_file"))?,
                    ),
                    &config.data_dir,
                    now,
                    history_hours,
                )?,
                _ => anyhow::bail!("unsupported temperature source"),
            };
            for path in paths {
                println!("{}", path.display());
            }
        }
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
