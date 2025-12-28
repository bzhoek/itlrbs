use chrono::{Datelike, Local};
use id3rs::ID3rs;
use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_itunes_library::{ITLibMediaItem, ITLibrary};
use rbsqlx::Database;
use regex::{Captures, Regex};
use std::fs;
use tracing::{debug, error, info, warn};

pub struct Music {
  itl: Retained<ITLibrary>,
}

impl Default for Music {
  fn default() -> Self {
    let itl = unsafe {
      let version = NSString::from_str("1");
      ITLibrary::libraryWithAPIVersion_error(&version).expect("Failed to load library")
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
      playlists.iter().find(|pl| pl.name().isEqualToString(&name)).map(|pl| pl.items()).iter().flatten().collect()
    };
    items
  }

  pub fn all_items(&self) -> Vec<Retained<ITLibMediaItem>> {
    let items: Vec<_> = unsafe { self.itl.allMediaItems().iter().filter(|item| !item.isRatingComputed()).collect() };
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
    let path = unsafe { item.location() }.and_then(|url| url.path()).map(|path| path.to_string()).ok_or(())?;

    Ok(Song {
      path,
      rating,
    })
  }
}

impl Song {
  pub fn relative_path(&self) -> &str {
    let icloud = "/Mobile Documents/com~apple~CloudDocs";
    self.path.split_once(icloud).map(|x| x.1).unwrap_or(&self.path)
  }

  pub fn deezer_id(&self) -> Option<&str> {
    parse_filename(&self.path).and_then(|caps| caps.get(4).map(|id| id.as_str()))
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

pub async fn rate_music(items: Vec<Retained<ITLibMediaItem>>, database: &Database) {
  let songs: Vec<Song> = items.iter().flat_map(|item| item.try_into()).collect();

  let handles = songs
    .into_iter()
    .map(|song| {
      let database = database.clone();
      tokio::spawn(async move {
        process_song(song, database).await;
      })
    })
    .collect::<Vec<_>>();

  for handle in handles {
    handle.await.unwrap();
  }
}

pub async fn tag_music(items: Vec<Retained<ITLibMediaItem>>, database: &Database, tag: &str, set: &[&'static str]) {
  let songs: Vec<Song> = items.iter().flat_map(|item| item.try_into()).collect();

  let handles = songs
    .into_iter()
    .map(|song| {
      let tag = tag.to_string();
      let database = database.clone();
      let set = set.to_owned();
      tokio::spawn(async move {
        tag_song(song, database, tag, set).await;
      })
    })
    .collect::<Vec<_>>();

  for handle in handles {
    handle.await.unwrap();
  }
}

async fn tag_song(song: Song, mut database: Database, tag: String, set: Vec<&str>) {
  match (fs::exists(&song.path).ok(), song.deezer_id()) {
    (Some(exists), Some(dzid)) if exists => match database.content(dzid).await {
      Ok(content) => {
        let tags = database.content_tags(&content).await.unwrap_or_default();
        let names = tags.iter()
          .map(|t| t.Name.as_str())
          .filter(|name| name != &tag && set.contains(name))
          .collect::<Vec<_>>();
        for name in names {
          info!("Remove tag {} from {}", name, song.relative_path());
          database.untag_content(&content, name).await.unwrap();
        }
        if let Some(usn) = database.tag_content(&content, &tag).await.unwrap() {
          info!("Tagged {} with {} usn {}", song.relative_path(), tag, usn);
        }
      }
      Err(_) => error!("Not in rekordbox {} with {:?}", song.relative_path(), dzid),
    },
    (Some(exists), _) if !exists => error!("Does not exist {}", song.path),
    _ => {}
  }
}

async fn process_song(song: Song, mut database: Database) {
  if song.rating == 0 {
    return;
  }

  match (fs::exists(&song.path).ok(), song.deezer_id()) {
    (Some(exists), _) if exists && song.rating == 1 => {
      warn!("Delete {} with {} star rating", song.relative_path(), song.rating);
      fs::remove_file(&song.path).unwrap();
    }
    (Some(exists), Some(dzid)) if exists => {
      match database.content(dzid).await {
        Ok(content) => {
          if song.rating > 0 && content.Rating == 0 {
            info!("Rating {} in rekordbox as {}", song.relative_path(), song.rating);
            database.rate_content(&content, song.rating as u8).await.unwrap();
          } else if song.rating > 0 && song.rating != content.Rating as usize {
            warn!(
              "Different rating for {} in Music {} and rekordbox {}",
              song.relative_path(),
              song.rating,
              content.Rating
            );
          }
        }
        Err(_) => error!("Not in rekordbox {} with {:?}", song.relative_path(), dzid),
      }
      update_id3(&song).await;
    }
    (Some(exists), _) if !exists => error!("Does not exist {}", song.path),
    _ => error!("Does not exist {}", song.path),
  }

  async fn update_id3(song: &Song) {
    let rate_song = |id3: &mut ID3rs, author| {
      id3.set_popularity(author, song.rating as u8);
      if id3.grouping().is_none() {
        id3.set_grouping(&year_week());
      }
      id3.write().unwrap_or_else(|_| error!("Failed to write {}", song.relative_path()));
    };

    match ID3rs::read(&song.path) {
      Err(_) => error!("Cannot read ID3 for {}", song.path),
      Ok(mut id3) => {
        for author in ["itunes", "traktor@native-instruments.de"].iter() {
          match id3.popularity(author) {
            Some((_, rating)) if rating != song.rating as u8 => {
              info!(
                "Update {} from Music {} over ID3 {} by {}",
                song.relative_path(),
                song.rating,
                rating,
                author );
              rate_song(&mut id3, author);
              return;
            }
            Some((_, _)) => return,
            _ => debug!( "No rating for {} by {}", song.relative_path(), author)
          }
        }
        info!("Rate {} from Music {}", song.relative_path(), song.rating);
        rate_song(&mut id3, "itunes");
      }
    }
  }
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
    assert_eq!(
      "/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3",
      song.path
    );
    assert_eq!("/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.relative_path());
    assert_eq!(3, song.rating);
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
  async fn test_sqlcipher() {
    let music = Music::default();
    let items = music.all_items();
    let database = &Database::connect("test_master.db").await.unwrap();
    rate_music(items, database).await;
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
      Ok(_) => match ID3rs::read(&path) {
        Ok(_) => {}
        Err(_) => error!("Cannot read {}", path),
      },
      Err(_) => error!("{} does not exist", path),
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
