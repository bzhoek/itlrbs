use chrono::{Datelike, Local};
use id3rs::ID3rs;
use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSString};
use objc2_itunes_library::{ITLibMediaItem, ITLibPlaylist, ITLibrary};
use rbsqlx::{Content, Database};
use regex::Regex;
use std::fs;
use std::sync::OnceLock;
use tracing::{debug, error, info, trace, warn};

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

  pub fn all_items(&self) -> Vec<Retained<ITLibMediaItem>> {
    unsafe {
      self.itl.allMediaItems().iter()
        .filter(|item| !item.isRatingComputed())
        .collect()
    }
  }

  pub fn all_items_by_filepath(&self, filepath: &str) -> Vec<Retained<ITLibMediaItem>> {
    let filepath = NSString::from_str(filepath);
    unsafe {
      self.itl.allMediaItems().iter()
        .filter(|item| {
          item.location()
            .and_then(|url| url.path())
            .map(|path| path.hasSuffix(&filepath))
            .unwrap_or(false)
        })
        .collect::<Vec<_>>()
    }
  }

  pub fn all_items_by_title(&self, title: &str) -> Vec<Retained<ITLibMediaItem>> {
    let title = NSString::from_str(title);
    unsafe { self.itl.allMediaItems() }.iter()
      .filter(|item| unsafe { item.title().isEqualToString(&title) })
      .collect::<Vec<_>>()
  }

  pub fn playlist_items(&self, name: &str) -> Vec<Retained<ITLibMediaItem>> {
    self.playlist_items_iter(name).flatten().collect()
  }

  pub fn playlist_item(&self, name: &str, needle: &Retained<ITLibMediaItem>) -> Vec<Retained<ITLibMediaItem>> {
    unsafe {
      self.playlist_items_iter(name).flatten()
        .filter(|it| it.persistentID() == needle.persistentID())
        .collect::<Vec<_>>()
    }
  }

  pub fn all_songs(&self) -> Vec<Song> {
    Music::map_songs(&self.all_items())
  }

  pub fn map_songs(items: &[Retained<ITLibMediaItem>]) -> Vec<Song> {
    items.iter().flat_map(|item| item.try_into()).collect()
  }

  fn playlist_items_iter(&self, name: &str) -> impl Iterator<Item=Retained<NSArray<ITLibMediaItem>>> {
    unsafe {
      self.playlist_by_name(name)
        .map(|pl| pl.items()).into_iter()
    }
  }

  fn playlist_by_name(&self, name: &str) -> Option<Retained<ITLibPlaylist>> {
    let name = NSString::from_str(name);
    unsafe {
      self.itl.allPlaylists().iter()
        .find(|pl| pl.name().isEqualToString(&name))
    }
  }

  pub fn one_item(mut items: Vec<Retained<ITLibMediaItem>>) -> Result<Retained<ITLibMediaItem>, SelectionError> {
    match items.len() {
      0 => Err(SelectionError::Empty),
      n if n > 1 => Err(SelectionError::TooMany(n)),
      _ => Ok(items.pop().unwrap()),
    }
  }
}

#[derive(Debug)]
pub enum SelectionError {
  Empty,
  TooMany(usize),
}

impl std::error::Error for SelectionError {}

impl std::fmt::Display for SelectionError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SelectionError::Empty => write!(f, "No items found"),
      SelectionError::TooMany(n) => write!(f, "Too many items found: {}", n),
    }
  }
}

pub struct Song {
  pub path: String,
  pub rating: usize,
  pub bpm: usize,
  pub grouping: Option<String>
}

impl TryFrom<&Retained<ITLibMediaItem>> for Song {
  type Error = ();

  fn try_from(item: &Retained<ITLibMediaItem>) -> Result<Self, Self::Error> {
    let rating = unsafe { item.rating() }.cast_unsigned() / 20;
    let bpm = unsafe { item.beatsPerMinute() };
    let grouping = unsafe { item.grouping() }.map(|s| s.to_string());
    let path = unsafe { item.location() }.and_then(|url| url.path()).map(|path| path.to_string()).ok_or(())?;

    Ok(Song {
      path,
      rating,
      bpm,
      grouping,
    })
  }
}

impl Song {
  pub fn relative_path(&self) -> &str {
    let icloud = "/Mobile Documents/com~apple~CloudDocs/Music";
    self.path.split_once(icloud).map(|x| x.1).unwrap_or(&self.path)
  }

  pub fn deezer_id(&self) -> Option<&str> {
    filename_re().captures(&self.path)
      .and_then(|caps| caps.get(4).map(|id| id.as_str()))
  }
}

fn filename_re() -> &'static Regex {
  static FILENAME_RE: OnceLock<Regex> = OnceLock::new();
  FILENAME_RE.get_or_init(|| Regex::new(r"^(?:(\d+)\.\s)?(.+)\s--\s(.+)?\s\[(\d+)]\.mp3$").unwrap())
}

pub async fn group_music(songs: Vec<Song>, database: &Database, dry_run: bool, force: bool) {
  let handles = songs
    .into_iter()
    .map(|song| {
      let database = database.clone();
      tokio::spawn(async move {
        group_song(song, database, dry_run, force).await;
      })
    })
    .collect::<Vec<_>>();

  for handle in handles {
    handle.await.unwrap();
  }
}

pub async fn rate_music(songs: Vec<Song>, database: &Database, dry_run: bool, force: bool) {
  let handles = songs
    .into_iter()
    .map(|song| {
      let database = database.clone();
      tokio::spawn(async move {
        rate_song(song, database, dry_run, force).await;
      })
    })
    .collect::<Vec<_>>();

  for handle in handles {
    handle.await.unwrap();
  }
}

pub async fn tag_music(songs: Vec<Song>, database: &Database, tag: &str, set: &[&'static str], dry_run: bool) {
  let handles = songs
    .into_iter()
    .map(|song| {
      let tag = tag.to_string();
      let database = database.clone();
      let set = set.to_owned();
      tokio::spawn(async move {
        tag_song(song, database, tag, set, dry_run).await;
      })
    })
    .collect::<Vec<_>>();

  for handle in handles {
    handle.await.unwrap();
  }
}

async fn group_song(song: Song, database: Database, dry_run: bool, force: bool) {
  match (fs::exists(&song.path).ok(), song.deezer_id()) {
    (Some(exists), Some(dzid)) if exists => match database.content(dzid).await {
      Ok(_) => {
        info!("Found {}", dzid);
      }
      Err(_) => warn!(r#"Not in rekordbox "{}" with {:?}"#, song.relative_path(), dzid),
    }
    (Some(exists), _) if !exists => error!("File does not exist {}", song.path),
    _ => {}
  }
}

async fn tag_song(song: Song, database: Database, tag: String, set: Vec<&str>, dry_run: bool) {
  match (fs::exists(&song.path).ok(), song.deezer_id()) {
    (Some(exists), Some(dzid)) if exists => match database.content(dzid).await {
      Ok(content) => {
        let tags = database.content_tags(&content).await.unwrap();
        let names = tags.iter()
          .map(|t| t.Name.as_str())
          .collect::<Vec<_>>();
        let removes = names.clone().into_iter()
          .filter(|name| name != &tag && set.contains(name))
          .collect::<Vec<_>>();
        for name in removes {
          if dry_run {
            info!(r#"Would remove tag {} from "{}""#, name, song.relative_path());
          } else {
            info!(r#"Remove tag {} from "{}""#, name, song.relative_path());
            database.untag_content(&content, name).await.unwrap();
          }
        }

        if dry_run && !names.contains(&&*tag) {
          info!(r#"Would tag "{}" with {}"#, song.relative_path(), tag);
        } else if let Some(usn) = database.tag_content(&content, &tag).await.unwrap() {
          info!(r#"Tagged "{}" with {} usn {}"#, song.relative_path(), tag, usn);
        }
      }
      Err(_) => warn!(r#"Not in rekordbox "{}" with {:?}"#, song.relative_path(), dzid),
    },
    (Some(exists), _) if !exists => error!("File does not exist {}", song.path),
    _ => {}
  }
}

async fn rate_song(song: Song, database: Database, dry_run: bool, force: bool) {
  if song.rating == 0 {
    return;
  }

  match (fs::exists(&song.path).ok(), song.deezer_id()) {
    (Some(exists), _) if exists && song.rating == 1 => {
      if dry_run {
        info!(r#"Would delete "{}" with {} star rating"#, song.relative_path(), song.rating);
      } else {
        warn!(r#"Delete "{}" with {} star rating"#, song.relative_path(), song.rating);
        fs::remove_file(&song.path).unwrap();
      }
    }
    (Some(exists), Some(dzid)) if exists => {
      match database.content(dzid).await {
        Ok(content) if force => {
          if dry_run {
            info!(r#"Would rate "{}" over rekordbox {} with {}"#, song.relative_path(), content.Rating, song.rating);
          } else {
            info!(r#"Force rating "{}" of rekordbox {} with {}"#, song.relative_path(), content.Rating, song.rating);
            database.rate_content(&content, song.rating as u8).await.unwrap();
          }
        }
        Ok(content) => {
          if song.rating > 0 && content.Rating == 0 {
            if dry_run {
              info!(r#"Would rate "{}" in rekordbox as {}"#, song.relative_path(), song.rating);
            } else {
              info!(r#"Rating "{}" in rekordbox as {}"#, song.relative_path(), song.rating);
              database.rate_content(&content, song.rating as u8).await.unwrap();
            }
          } else if song.rating > 0 && song.rating != content.Rating as usize {
            warn!(
              r#"Clash on "{}" with Music {} and rekordbox {} rating"#,
              song.relative_path(),
              song.rating,
              content.Rating
            );
          }
        }
        Err(_) => warn!(r#"Not in rekordbox "{}" with {:?}"#, song.relative_path(), dzid),
      }
      update_id3(&song, dry_run, force).await;
    }
    (_, None) => debug!("No Deezer ID {}", song.path),
    _ => error!("Does not exist {}", song.path),
  }

  async fn update_id3(song: &Song, dry_run: bool, force: bool) {
    let rate_song = |id3: &mut ID3rs, author| {
      id3.clear_popularities();
      id3.set_popularity(author, song.rating as u8);
      if id3.grouping().is_none() {
        id3.set_grouping(&year_week());
      }
      if dry_run { return; }
      id3.write().unwrap_or_else(|_| error!(r#"Failed to write "{}""#, song.relative_path()));
    };

    match ID3rs::read(&song.path) {
      Err(_) => error!("Cannot read ID3 for {}", song.path),
      Ok(mut id3) => {
        for author in ["itunes", "traktor@native-instruments.de"].iter() {
          match id3.popularity(author) {
            Some((_, rating)) if rating != song.rating as u8 || force => {
              info!(
                r#"Update "{}" with Music {} over ID3 {} as '{}'"#,
                song.relative_path(),
                song.rating,
                rating,
                author );
              rate_song(&mut id3, author);
              return;
            }
            Some((_, _)) => return,
            _ => trace!(r#"No rating for "{}" by {}"#, song.relative_path(), author)
          }
        }
        info!(r#"Rate "{}" from Music {}"#, song.relative_path(), song.rating);
        rate_song(&mut id3, "itunes");
      }
    }
  }
}

fn year_week() -> String {
  let today = Local::now().date_naive();
  let iso_week = today.iso_week();
  let week_number = iso_week.week();
  format!("{:02}{:02}", iso_week.year() % 100, week_number)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{Datelike, NaiveDate};
  use id3rs::ID3rs;
  use rbsqlx::{Content, Database};
  use std::fs;

  #[test]
  fn test_playlist_items() {
    let music = Music::default();
    let items = music.playlist_items("eatmos");
    assert!(items.len() > 500, "Found only {} items", items.len());
    let item = items.first().unwrap();
    let song: Song = item.try_into().unwrap();
    assert_eq!(
      "/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3",
      song.path
    );
    assert_eq!("/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3", song.relative_path());
    assert_eq!(3, song.rating);
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
  async fn test_rating_songs() {
    tracing_subscriber::fmt::init();
    let music = Music::default();
    let database = &Database::connect("test_master.db").await.unwrap();
    let songs = music.all_songs();
    rate_music(songs, database, true, false).await;
  }

  #[tokio::test]
  async fn test_content_tags() {
    let mut database = Database::connect("test_master.db").await.unwrap();
    let content = Content { ID: "68739521".into(), FileNameL: "0. Eviction -- Linea Aspera [1082461272].mp3".into(), Rating: 3 };
    let tags = database.content_tags(&content).await.unwrap();
    assert_eq!(1, tags.len());
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
    let song = Song {
      path: "/Users/bas/Library/Mobile Documents/com~apple~CloudDocs/Music/discover/DW202123/29. 2020 Souls -- Aaaron [918205852].mp3".to_string().into(),
      rating: 3,
      bpm: 120,
      grouping: None,
    };
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
    assert!(items.len() > 6000, "Found only {} items", items.len());
  }

  #[test]
  fn test_all_songs_len() {
    let music = Music::default();
    let items = music.all_items();
    let songs = music.all_songs();
    assert!(items.len() - songs.len() < 10, "Found {} items and {} songs", items.len(), songs.len());
  }

  #[test]
  fn test_week_number() {
    let today = NaiveDate::from_ymd_opt(2025, 12, 9).unwrap();
    let iso_week = today.iso_week();
    let week_number = iso_week.week();
    let year_week = format!("{:02}{:02}", iso_week.year() % 100, week_number);
    assert_eq!("2550", year_week);
  }
}
