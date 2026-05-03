mod cli;
use itlrbs::{group_music, rate_music, tag_music, Music};
use rbsqlx::Database;
use tracing::info;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let cli = cli::parse_cli();
  let _guard = cli::setup_logger(cli.verbose);

  let url = cli.database.to_str().expect("invalid database path");
  let database = &mut Database::connect(url).await?;
  info!("Database connected: {}", url);
  if cli.dry_run {
    info!("Dry run mode, not making actual changes");
  }

  let music = Music::default();
  if let Some(cli::Command::Group { filename: filepath }) = &cli.command {
    let items = music.all_items_by_filepath(filepath);
    let item = Music::one_item(items)?;

    info!(r#"Grouping filepath "{}""#, filepath);
    let songs = Music::map_songs(&[item]);
    group_music(songs, database, cli.dry_run, true).await;
    database.checkpoint().await?;
    return Ok(());
  }
  if let Some(cli::Command::Rate { filename: filepath, rating }) = &cli.command {
    let items = music.all_items_by_filepath(filepath);
    let item = Music::one_item(items)?;

    info!(r#"Tagging filepath "{}" with rating {}"#, filepath, rating);
    let songs = Music::map_songs(&[item]);
    rate_music(songs, database, cli.dry_run, true).await;
    database.checkpoint().await?;
    return Ok(());
  }

  // use the same item for rating and tagging if specified
  let one = match &cli.command {
    Some(cli::Command::Title { title }) => {
      let items = music.all_items_by_title(title);
      Music::one_item(items)?.into()
    },
    _ => None,
  };

  let items = match &one {
    Some(item) => vec![item.clone()],
    _ => music.all_items(),
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
      let items = match &one {
        Some(item) => music.playlist_item(list, item),
        None => music.playlist_items(list)
      };
      info!("Tagging {} songs with '{}'", items.len(), list);
      let songs = Music::map_songs(&items);
      tag_music(songs, database, list, &set, cli.dry_run).await;
    }
  }

  database.checkpoint().await?;
  Ok(())
}
