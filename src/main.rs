use itlrbs::{Music, Song};

fn main() {
  let lists = ["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"];
  let music = Music::default();
  let items = music.all_songs();
  println!("Version {} has {} songs", music.version(), items.len());
  for list in lists.into_iter() {
    let items = music.playlist_items(list);
    print!("{:>6}: {} songs", list, items.len());
    let song: Song = items.first().unwrap().into();
    println!(", first {} {}", song.relative_path().unwrap(), "*".repeat(song.rating));
  }
}