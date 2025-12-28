use clap::{Parser, Subcommand};
use itlrbs::{rate_music, setup_logger, tag_music, Music};
use rbsqlx::Database;
use std::path::PathBuf;
use tracing::{error, info};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
  /// don't make actual changes
  #[arg(short, long)]
  dry_run: bool,
  /// verbose logging
  #[arg(short, long)]
  verbose: bool,
  /// rekordbox master.db path
  database: PathBuf,
  /// song title to process
  title: Option<String>,
  #[command(subcommand)]
  pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
  /// rate a single file directly
  Rate {
    /// full filename to rate
    filename: String,
    /// rating to set
    rating: u8,
  },
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
  let cli = Cli::parse();

  let _guard = setup_logger(cli.verbose);

  let url = cli.database.to_str().expect("invalid database path");
  let database = &mut Database::connect(url).await.unwrap();
  info!("Database connected: {}", url);
  if cli.dry_run {
    info!("Dry run mode, not making actual changes");
  }

  let music = Music::default();
  if let Some(Command::Rate { filename, rating }) = &cli.command {
    let items = music.all_items_by_filepath(filename);
    match items.len() {
      0 => info!("No songs found for filename '{}'", filename),
      1 => {
        info!("Tagging {} song with rating {}", items.len(), rating);
        let songs = Music::map_songs(&items);
        rate_music(songs, database, cli.dry_run, true).await;
        database.checkpoint().await.unwrap();
      }
      _ => error!("Not unique filename '{}', {:?}", filename, items),
    }
    return;
  }

  let items = match &cli.title {
    Some(title) => music.all_items_by_title(title),
    None => music.all_items()
  };
  info!("Version {} for {} songs", music.version(), items.len());
  let songs = Music::map_songs(&items);
  rate_music(songs, database, cli.dry_run, false).await;

  let sets = vec![
    vec!["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"],
    vec!["vocals"],
  ];
  for set in sets {
    for list in set.iter() {
      let items = match &cli.title {
        Some(title) => music.playlist_items_by_title(list, title),
        None => music.playlist_items(list)
      };
      info!("Tagging {} songs with '{}'", items.len(), list);
      let songs = Music::map_songs(&items);
      tag_music(songs, database, list, &set, cli.dry_run).await;
    }
  }

  database.checkpoint().await.unwrap();
}