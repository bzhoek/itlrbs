use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_itunes_library::{ITLibMediaItem, ITLibrary};

struct Music {
  itl: Retained<ITLibrary>,
}

impl Music {
  fn new() -> Self {
    let itl = unsafe {
      let version = NSString::from_str("1");
      ITLibrary::libraryWithAPIVersion_error(&version)
        .expect("Failed to load library")
    };
    Music { itl }
  }

  fn version(&self) -> String {
    unsafe { self.itl.applicationVersion().to_string() }
  }

  fn playlist_items(&self, name: &str) -> Vec<Retained<ITLibMediaItem>> {
    let playlists = unsafe { self.itl.allPlaylists() };
    let name = NSString::from_str(name);
    let items: Vec<_> = unsafe {
      playlists
        .iter()
        .find(|pl| pl.name().isEqualToString(&name))
        .map(|pl| pl.items())
        .iter().flatten().collect()
    };
    items
  }

  fn all_items(&self) -> Vec<Retained<ITLibMediaItem>> {
    let items: Vec<_> = unsafe {
      self.itl.allMediaItems()
        .iter().filter(|item| !item.isRatingComputed())
        .collect()
    };
    items
  }

  fn all_songs(&self) -> Vec<Song> {
    self.all_items().iter().map(|item| item.into()).collect()
  }
}

struct Song {
  path: Option<String>,
  rating: usize,
}

impl From<&Retained<ITLibMediaItem>> for Song {
  fn from(item: &Retained<ITLibMediaItem>) -> Self {
    let path = unsafe { item.location() }
      .and_then(|url| url.path())
      .map(|path| path.to_string());
    let rating = unsafe { item.rating() }.cast_unsigned() / 20;
    Song { path, rating }
  }
}

impl Song {
  fn relative_path(&self) -> Option<&str> {
    let icloud = "/Mobile Documents/com~apple~CloudDocs";
    self.path.as_ref()
      .and_then(|path| path.split_once(icloud))
      .map(|x| x.1)
  }
}

fn main() {
  let lists = ["eatmos", "ebup", "edrive", "epeak", "ebang", "ebdown"];
  let music = Music::new();
  let items = music.all_songs();
  println!("Version {} has {} songs", music.version(), items.len());
  for list in lists.into_iter() {
    let items = music.playlist_items(list);
    print!("{:>6}: {} songs", list, items.len());
    let song: Song = items.first().unwrap().into();
    println!(", first {} {}", song.relative_path().unwrap(), "*".repeat(song.rating));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_playlist_items() {
    let music = Music::new();
    let items = music.playlist_items("eatmos");
    assert_eq!(552, items.len());
    let item = items.first().unwrap();
    let song: Song = item.into();
    assert_eq!("/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.path.as_ref().unwrap());
    assert_eq!("/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.relative_path().unwrap());
    assert_eq!(3, song.rating);
  }

  #[test]
  fn test_all_items() {
    let music = Music::new();
    let items = music.all_items();
    assert_eq!(6985, items.len());
  }

  #[test]
  fn test_all_songs() {
    let music = Music::new();
    let items = music.all_songs();
    assert_eq!(6985, items.len());
  }
}