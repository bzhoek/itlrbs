use rbsqlx::Database;
use itlrbs::{tag_music, Music, Song};

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() {
  let database = &Database::connect("test_master.db").await.unwrap();

  let lists = ["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"];
  let music = Music::default();
  let items = music.all_songs();
  println!("Version {} has {} songs", music.version(), items.len());

  for list in lists.into_iter() {
    let items = music.playlist_items(list);
    println!("{:>6}: {} songs", list, items.len());
    tag_music(items, database, list).await;
  }
}