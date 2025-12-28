use clap::Parser;
use itlrbs::{rate_music, tag_music, Music};
use rbsqlx::Database;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
  /// don't make actual changes
  #[arg(short, long)]
  dry_run: bool,
  /// rekordbox master.db file
  database: PathBuf,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
  let cli = Cli::parse();

  tracing_subscriber::fmt::init();

  let url = cli.database.to_str().expect("invalid database path");
  let database = &mut Database::connect(url).await.unwrap();
  info!("Database connected: {}", url);
  if cli.dry_run {
    info!("Dry run mode, not making actual changes");
  }

  let music = Music::default();
  // TODO: make this an optional cli parameter
  // let name = NSString::from_str("1a E Vangelis Dreams (Baime Remix)");
  // let items = music.all_items().into_iter()
  //   .filter(|item| unsafe { item.title().isEqualToString(&*name) })
  //   .collect::<Vec<_>>();
  // let items = items.iter().flat_map(|item| item.try_into()).collect::<Vec<Song>>();
  let items = music.all_songs();
  info!("Version {} has {} songs", music.version(), items.len());
  rate_music(items, database, cli.dry_run).await;

  let sets = vec![
    vec!["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"],
    vec!["vocals"],
  ];
  for set in sets {
    for list in set.iter() {
      let items = music.playlist_items(list);
      info!("Tagging {} songs with '{}'", items.len(), list);
      tag_music(items, database, list, &set, cli.dry_run).await;
    }
  }

  database.checkpoint().await.unwrap();
}