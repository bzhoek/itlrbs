use chrono::{Datelike, Local};
use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_itunes_library::{ITLibMediaItem, ITLibrary};
use regex::{Captures, Regex};

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
    self.all_items().iter().flat_map(|item| item.try_into()).collect()
  }
}

pub struct Song {
  pub path: String,
  pub rating: usize,
}

impl TryFrom<&Retained<ITLibMediaItem>> for Song {
  type Error = ();

  fn try_from(item: &Retained<ITLibMediaItem>) -> Result<Self, Self::Error> {
    let rating = unsafe { item.rating() }.cast_unsigned() / 20;
    let path = unsafe { item.location() }
      .and_then(|url| url.path())
      .map(|path| path.to_string())
      .ok_or(())?;

    Ok(Song { path, rating })
  }
}

impl Song {
  pub fn relative_path(&self) -> &str {
    let icloud = "/Mobile Documents/com~apple~CloudDocs";
    self.path.split_once(icloud).map(|x| x.1).unwrap_or(&self.path)
  }

  pub fn deezer_id(&self) -> Option<&str> {
    parse_filename(&self.path)
      .and_then(|caps| caps.get(4).map(|id| id.as_str()))
  }
}

pub fn parse_filename(filename: &str) -> Option<Captures<'_>> {
  let re = Regex::new(r"^(?:(\d+)\.\s)?(.+)\s--\s(.+)?\s\[(\d+)]\.mp3$").unwrap();
  re.captures(filename)
}

#[allow(unused)]
fn year_week() -> String {
  let today = Local::now().date_naive();
  let iso_week = today.iso_week();
  let week_number = iso_week.week();
  format!("{:02}{:02}", iso_week.year() % 100, week_number)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{Datelike, Local};
  use id3rs::ID3rs;
  use rbsqlx::Database;
  use std::fs;

  #[test]
  fn test_playlist_items() {
    let music = Music::default();
    let items = music.playlist_items("eatmos");
    assert_eq!(553, items.len());
    let item = items.first().unwrap();
    let song: Song = item.try_into().unwrap();
    assert_eq!("/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.path);
    assert_eq!("/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.relative_path());
    assert_eq!(3, song.rating);
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
  async fn test_sqlcipher() {
    let database = Database::connect("test_master.db").await.unwrap();

    let music = Music::default();
    let items = music.all_items();
    let songs: Vec<Song> = items.iter().flat_map(|item| item.try_into()).collect();

    let handles = songs.into_iter().map(|song| {
      let database = database.clone();
      tokio::spawn(async move {
        process_song(song, database).await;
      })
    }).collect::<Vec<_>>();

    for handle in handles {
      handle.await.unwrap();
    }
  }

  async fn process_song(song: Song, mut conn: Database) {
    match (fs::exists(&song.path).ok(), song.deezer_id()) {
      (Some(exists), _) if exists && song.rating == 1 => {
        // fs::remove_file(&song.path).unwrap();
        eprintln!("Delete {} with {} star rating", song.relative_path(), song.rating);
      }
      (Some(exists), Some(dzid)) if exists => {
        match conn.content(dzid).await {
          Ok(content) => {
            if song.rating > 0 && content.Rating == 0 {
              eprintln!("Rating {} in rekordbox as {}", song.relative_path(), song.rating);
            } else if song.rating > 0 && song.rating != content.Rating as usize {
              eprintln!("Different rating for {} in Music {} and rekordbox {}", song.relative_path(), song.rating, content.Rating);
            }
          }
          Err(_) => eprintln!("Not in rekordbox {} with {:?}", song.relative_path(), dzid)
        }
        // match ID3rs::read(&song.path) {
        //   Ok(mut id3) => {
        //     match id3.popularity("itunes") {
        //       Some((author, rating)) if rating != song.rating as u8 => {
        //         eprintln!("Different rating for {} in Music {} and ID3 {} by {}", song.relative_path(), song.rating, rating, author);
        //         id3.set_popularity("itunes", song.rating as u8);
        //         id3.set_grouping(&year_week());
        //         id3.write().expect(format!("Failed to write {}", song.relative_path()).as_str());
        //       }
        //       _ => {}
        //     }
        //   }
        //   Err(_) => eprintln!("Cannot read ID3 for {}", song.path),
        // }
      }
      (Some(exists), _) if !exists => eprintln!("Does not exist {}", song.path),
      _ => {}
    }
  }

  #[test]
  fn test_process_all() {
    // let rb = Arc::new(RwLock::new(happer::rekordbox::Rekordbox::new("test_master.db").unwrap()));
    let music = Music::default();
    let items = music.all_items();
    let songs: Vec<Song> = items.iter().flat_map(|item| item.try_into()).collect();
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

  fn process(song: Song) {
    let path = song.path;
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
    assert_eq!(6984, items.len());
  }

  #[test]
  fn test_week_number() {
    let today = Local::now().date_naive();
    let iso_week = today.iso_week();
    let week_number = iso_week.week();
    let year_week = format!("{:02}{:02}", iso_week.year() % 100, week_number);
    assert_eq!("2550", year_week);
  }
}
