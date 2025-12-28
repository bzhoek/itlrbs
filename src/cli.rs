use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub fn parse_cli() -> Cli {
  Cli::parse()
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
  /// don't make actual changes
  #[arg(short, long)]
  pub(crate) dry_run: bool,
  /// verbose logging
  #[arg(short, long)]
  pub(crate) verbose: bool,
  /// rekordbox master.db path
  pub(crate) database: PathBuf,
  #[command(subcommand)]
  pub command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
  /// process a single file
  Title {
    /// song title
    title: String,
  },
  /// rate a single file directly
  Rate {
    /// full filename to rate
    filename: String,
    /// rating to set
    rating: u8,
  },
}

pub fn setup_logger(verbose: bool) -> WorkerGuard {
  let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| {
      let directives = format!("{},id3rs=info,sqlx=info", if verbose { "debug" } else { "info" });
      EnvFilter::new(directives)
    });
  let file_appender = tracing_appender::rolling::daily("logs", "itlrbs.log");
  let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

  tracing_subscriber::registry()
    .with(filter)
    .with(tracing_subscriber::fmt::layer())
    .with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false))
    .init();

  guard
}
