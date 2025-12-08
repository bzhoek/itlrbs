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

#[cfg(test)]
mod tests {
  use super::*;
  use dotenvy::dotenv;
  use id3rs::ID3rs;
  use r2d2::{Error, Pool, PooledConnection};
  use r2d2_sqlite::SqliteConnectionManager;
  use rayon::iter::{IntoParallelIterator, ParallelIterator};
  use rusqlite::{Params, Row};
  use std::{env, fs};

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

  #[test]
  fn test_sqlcipher() {
    let pool = pool_for("test_master.db").unwrap();
    let conn = pool.get().unwrap();

    let count: i64 = conn.query_row(
      "SELECT COUNT(*) FROM djmdContent",
      [],
      |row| row.get(0),
    ).unwrap();
    assert_eq!(7347, count);
    let id: String = conn.query_row(
      "SELECT * FROM djmdContent WHERE FileNameL like ?",
      [format!("%[{}]%", "918205852")],
      |row| row.get(0),
    ).unwrap();
    assert_eq!("43970339", id);

    let music = Music::default();
    let items = music.all_items();
    let songs: Vec<Song> = items.iter().flat_map(|item| item.try_into()).collect();
    // songs.into_iter().for_each(|song| {
    //   process_db(song, pool.get().unwrap());
    // });
    songs.into_par_iter().for_each(|song| {
      process_db(song, pool.get().unwrap());
    });
  }

  fn pool_for(path: &str) -> Result<Pool<SqliteConnectionManager>, Error> {
    dotenv().ok();
    let manager = SqliteConnectionManager::file(path)
      .with_init(|conn| {
        let pragma = format!("PRAGMA key = '{}';", env::var("SQLCIPHER_KEY").unwrap());
        conn.execute_batch(pragma.as_str())
      });

    Pool::new(manager)
  }
  struct Content {
    id: String,
    rating: usize,
  }

  impl TryFrom<&Row<'_>> for Content {
    type Error = rusqlite::Error;

    fn try_from(value: &Row<'_>) -> Result<Self, Self::Error> {
      Ok(Content {
        id: value.get(0)?,
        rating: value.get(15)?,
      })
    }
  }
  pub fn query_one<T, P>(
    conn: &PooledConnection<SqliteConnectionManager>,
    sql: &str,
    params: P,
  ) -> rusqlite::Result<T>
  where
    P: Params,
    for<'r> T: TryFrom<&'r Row<'r>, Error=rusqlite::Error>,
  {
    conn.query_row(sql, params, |row| T::try_from(row))
  }

  fn process_db(song: Song, conn: PooledConnection<SqliteConnectionManager>) {
    match (fs::exists(&song.path).ok(), song.deezer_id()) {
      (Some(exists), _) if exists && song.rating == 1 => {
        // fs::remove_file(&song.path).unwrap();
        eprintln!("Delete {} with {} star rating", song.relative_path(), song.rating);
      }
      (Some(exists), Some(dzid)) if exists => {
        let content: Result<Content, _> = query_one(&conn, "SELECT * FROM djmdContent WHERE FileNameL like ?", [format!("%[{}]%", dzid)]);
        match content {
          Ok(content) => {
            if song.rating > 0 && content.rating == 0 {
              eprintln!("Rating {} in rekordbox as {}", song.relative_path(), song.rating);
            } else if song.rating > 0 && song.rating != content.rating {
              eprintln!("Different rating for {} in Music {} and rekordbox {}", song.relative_path(), song.rating, content.rating);
            }
          }
          Err(_) => eprintln!("Not in rekordbox {} with {:?}", song.relative_path(), dzid)
        }
        match ID3rs::read(&song.path) {
          Ok(_) => {}
          Err(_) => eprintln!("Cannot read ID3 for {}", song.path),
        }
      }
      (Some(exists), _) if !exists => eprintln!("Does not exist {}", song.path),
      _ => {}
    }
  }

  // #[test]
  // fn test_master_db() {
  //   let rb = happer::rekordbox::Rekordbox::new("test_master.db").unwrap();
  //   let content = rb.with_deezer("918205852").unwrap();
  //   assert_eq!(3, content.Rating);
  //   assert_eq!("29. 2020 Souls -- Aaaron [918205852].mp3", content.FileNameL);
  // }

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

  #[test]
  fn test_par_process_all() {
    let music = Music::default();
    let items = music.all_items();
    let songs: Vec<Song> = items.iter().flat_map(|item| item.try_into()).collect();
    songs.into_par_iter().for_each(|song| {
      process(song);
    });
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
}
