use std::path::PathBuf;
use clap::{Parser, Subcommand};

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