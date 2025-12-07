use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_itunes_library::{ITLibMediaItem, ITLibrary};

pub struct Music {
  itl: Retained<ITLibrary>,
}

impl Default for Music {
  fn default() -> Self {
    let itl = unsafe {
      let version = NSString::from_str("1");
      ITLibrary::libraryWithAPIVersion_error(&version)
        .expect("Failed to load library")
    };
    Music { itl }
  }
}

impl Music {
  pub fn version(&self) -> String {
    unsafe { self.itl.applicationVersion().to_string() }
  }

  pub fn playlist_items(&self, name: &str) -> Vec<Retained<ITLibMediaItem>> {
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

  pub fn all_items(&self) -> Vec<Retained<ITLibMediaItem>> {
    let items: Vec<_> = unsafe {
      self.itl.allMediaItems()
        .iter().filter(|item| !item.isRatingComputed())
        .collect()
    };
    items
  }

  pub fn all_songs(&self) -> Vec<Song> {
    self.all_items().iter().map(|item| item.into()).collect()
  }
}

pub struct Song {
  pub path: Option<String>,
  pub rating: usize,
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
  pub fn relative_path(&self) -> Option<&str> {
    let icloud = "/Mobile Documents/com~apple~CloudDocs";
    self.path.as_ref()
      .and_then(|path| path.split_once(icloud))
      .map(|x| x.1)
  }

  pub fn deezer_id(&self) -> Option<&str> {
    self.path.as_ref().and_then(|path| happer::deezer::parse_filename(path))
      .and_then(|caps| caps.get(4).map(|id| id.as_str()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use id3rs::ID3rs;
  use rayon::iter::{IntoParallelIterator, ParallelIterator};
  use std::fs;

  #[test]
  fn test_playlist_items() {
    let music = Music::default();
    let items = music.playlist_items("eatmos");
    assert_eq!(552, items.len());
    let item = items.first().unwrap();
    let song: Song = item.into();
    assert_eq!("/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.path.as_ref().unwrap());
    assert_eq!("/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.relative_path().unwrap());
    assert_eq!(3, song.rating);
  }

  #[test]
  fn test_master_db() {
    let rb = happer::rekordbox::Rekordbox::new("test_master.db").unwrap();
    let content = rb.with_deezer("918205852").unwrap();
    assert_eq!(3, content.Rating);
    assert_eq!("29. 2020 Souls -- Aaaron [918205852].mp3", content.FileNameL);
  }

  #[test]
  fn test_process_all() {
    let music = Music::default();
    let items = music.all_items();
    let songs: Vec<Song> = items.iter().map(|item| item.into()).collect();
    songs.into_iter().for_each(|song| {
      process(song);
    });
  }

  #[test]
  fn test_deezer_id() {
    let song = Song { path: "/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3".to_string().into(), rating: 3 };
    let id = song.deezer_id().unwrap();
    assert_eq!("918205852", id);
  }

  #[test]
  fn test_par_process_all() {
    let music = Music::default();
    let items = music.all_items();
    let songs: Vec<Song> = items.iter().map(|item| item.into()).collect();
    songs.into_par_iter().for_each(|song| {
      process(song);
    });
  }

  fn process(song: Song) {
    if let Some(path) = song.path {
      match fs::exists(&path) {
        Ok(_) => {
          match ID3rs::read(&path) {
            Ok(_) => {}
            Err(_) => eprintln!("Cannot read {}", path),
          }
        }
        Err(_) => eprintln!("{} does not exist", path),
      };
    }
  }

  #[test]
  fn test_all_items_len() {
    let music = Music::default();
    let items = music.all_items();
    assert_eq!(6985, items.len());
  }

  #[test]
  fn test_all_songs_len() {
    let music = Music::default();
    let items = music.all_songs();
    assert_eq!(6985, items.len());
  }
}
