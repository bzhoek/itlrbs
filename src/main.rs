use std::path::PathBuf;
use clap::Parser;
use itlrbs::{tag_music, Music};
use rbsqlx::Database;
use tracing::info;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
  /// rekordbox master.db file
  database: PathBuf,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
  let cli = Cli::parse();

  tracing_subscriber::fmt::init();

  let url = cli.database.to_str().expect("invalid database path");
  let database = &Database::connect(url).await.unwrap();
  info!("Database connected: {}", url);

  let music = Music::default();
  let items = music.all_songs();
  info!("Version {} has {} songs", music.version(), items.len());

  let sets = vec![
    vec!["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"],
    vec!["vocals"],
  ];
  for set in sets {
    for list in set.iter() {
      let items = music.playlist_items(list);
      info!("{:>6}: {} songs", list, items.len());
      tag_music(items, database, list, &set).await;
    }
  }
}